/*
 * This file is part of the Trezor project, https://trezor.io/
 *
 * Copyright (c) SatoshiLabs
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */
#ifdef KERNEL_MODE

#include <sys/backup_ram.h>
#include <sys/irq.h>
#include <sys/pmic.h>
#include <sys/systick.h>
#include <trezor_rtl.h>

#include "../fuel_gauge/fuel_gauge.h"
#include "../stwlc38/stwlc38.h"
#include "power_manager_internal.h"

static void pm_battery_sampling(float vbat, float ibat, float ntc_temp);
static void pm_parse_power_source_state(pm_driver_t* drv);

void pm_monitor_power_sources(void) {
  // Periodically called timer to request PMIC measurements. PMIC will call
  // pm_pmic_data_ready() callback when the measurements are ready.
  pmic_measure(pm_pmic_data_ready, NULL);
}

// pmic measurement callback
void pm_pmic_data_ready(void* context, pmic_report_t* report) {
  pm_driver_t* drv = &g_pm;

  // Store measurement timestamp
  if (drv->pmic_last_update_us == 0) {
    drv->pmic_sampling_period_ms = PM_BATTERY_SAMPLING_PERIOD_MS;
  } else {
    // Calculate the time since the last PMIC update
    drv->pmic_sampling_period_ms =
        (systick_us() - drv->pmic_last_update_us) / 1000;
  }
  drv->pmic_last_update_us = systick_us();

  // Copy pmic data
  memcpy(&drv->pmic_data, report, sizeof(pmic_report_t));

  // Get wireless charger data
  stwlc38_get_report(&drv->wireless_data);

  pm_parse_power_source_state(drv);

  // Run battery charging controller
  pm_charging_controller(drv);

  if (!drv->fuel_gauge_initialized) {
    // Fuel gauge not initialized yet, battery SoC not available, sample the
    // battery data into the circular buffer.
    pm_battery_sampling(drv->pmic_data.vbat, drv->pmic_data.ibat,
                        drv->pmic_data.ntc_temp);

  } else {
    if (drv->woke_up_from_suspend) {
      // Just woke up from suspend, use the last known battery data to
      // update the fuel gauge.
      if (drv->suspended_charging) {
        pm_compensate_fuel_gauge(&drv->fuel_gauge.soc, drv->time_in_suspend_s,
                                 drv->pmic_data.ibat, drv->pmic_data.ntc_temp);

      } else {
        // Use known battery self-discharge rate to compensate the fuel gauge
        // estimation during the suspend period. Since this period may be very
        // long and the battery temperature may vary, use the average ambient
        // temperature.
        pm_compensate_fuel_gauge(&drv->fuel_gauge.soc, drv->time_in_suspend_s,
                                 PM_SELF_DISG_RATE_SUSPEND_MA, 25.0f);

        // TODO: Currently in suspend mode we use single self-discharge rate
        // but in practive the discharge rate may change in case the BLE chip
        // remains active. Since the device is very likely to stay in suspend
        // mode for limited time, for now we decided to neglect this. but in
        // the future we may want to distinguish between suspend mode
        // with/without BLE and use different self-discharge rates.
      }

      fuel_gauge_set_soc(&drv->fuel_gauge, drv->fuel_gauge.soc,
                         drv->fuel_gauge.P);

      // clear the flag
      drv->woke_up_from_suspend = false;

    } else {
      fuel_gauge_update(&drv->fuel_gauge, drv->pmic_sampling_period_ms,
                        drv->pmic_data.vbat, drv->pmic_data.ibat,
                        drv->pmic_data.ntc_temp);
    }

    // Charging completed
    if (drv->pmic_data.charge_status & 0x2) {
      // Force fuel gauge to 100%, keep the covariance
      fuel_gauge_set_soc(&drv->fuel_gauge, 1.0f, drv->fuel_gauge.P);
    }

    // Ceil the float soc to user-friendly integer
    drv->soc_ceiled = (uint8_t)(drv->fuel_gauge.soc_latched * 100 + 0.999f);

    // Check battery voltage for low threshold
    if (drv->soc_ceiled <= PM_BATTERY_LOW_THRESHOLD_SOC && !drv->battery_low) {
      drv->battery_low = true;
    } else if (drv->soc_ceiled > PM_BATTERY_LOW_THRESHOLD_SOC &&
               drv->battery_low) {
      drv->battery_low = false;
    }

    // Process state machine with updated battery and power source information
    pm_process_state_machine();

    pm_store_data_to_backup_ram();

    if (drv->suspending) {
      pm_schedule_rtc_wakeup();
      drv->suspending = false;
      drv->suspended = true;
    }

    drv->state_machine_stabilized = true;
  }
}

void pm_charging_controller(pm_driver_t* drv) {
  if (drv->charging_enabled == false) {
    // Charging is disabled
    if (drv->charging_current_target_ma != 0) {
      drv->charging_current_target_ma = 0;
    } else {
      // No action required
      return;
    }
  } else if (drv->usb_connected) {
    drv->charging_current_target_ma = PM_BATTERY_CHARGING_CURRENT_MAX;

  } else if (drv->wireless_connected) {
    drv->charging_current_target_ma = PM_BATTERY_CHARGING_CURRENT_MAX;

  } else {
    // Charging enabled but no external power source, clear charging target
    drv->charging_current_target_ma = 0;
  }

  // charging current software limit
  if (drv->charging_current_target_ma > drv->charging_current_max_limit_ma) {
    drv->charging_current_target_ma = drv->charging_current_max_limit_ma;
  }

  // Set charging target
  if (drv->charging_current_target_ma != pmic_get_charging_limit()) {
    // Set charging current limit
    pmic_set_charging_limit(drv->charging_current_target_ma);
  }

  if ((drv->soc_ceiled >= drv->soc_limit) && (drv->soc_limit != 100)) {
    drv->soc_limit_reached = true;
  } else if ((drv->soc_limit == 100) ||
             (drv->soc_ceiled < (drv->soc_limit - PM_SOC_LIMIT_HYSTERESIS))) {
    drv->soc_limit_reached = false;
  }

  if (drv->soc_limit_reached) {
    // Set charging current limit to 0
    drv->charging_current_target_ma = 0;
  }

  if (drv->charging_current_target_ma == 0) {
    pmic_set_charging(false);
  } else {
    pmic_set_charging(true);
  }
}

static void pm_battery_sampling(float vbat, float ibat, float ntc_temp) {
  pm_driver_t* drv = &g_pm;

  // Store battery data in the buffer
  drv->bat_sampling_buf[drv->bat_sampling_buf_head_idx].vbat = vbat;
  drv->bat_sampling_buf[drv->bat_sampling_buf_head_idx].ibat = ibat;
  drv->bat_sampling_buf[drv->bat_sampling_buf_head_idx].ntc_temp = ntc_temp;

  // Update head index
  drv->bat_sampling_buf_head_idx++;
  if (drv->bat_sampling_buf_head_idx >= PM_BATTERY_SAMPLING_BUF_SIZE) {
    drv->bat_sampling_buf_head_idx = 0;
  }

  // Check if the buffer is full
  if (drv->bat_sampling_buf_head_idx == drv->bat_sampling_buf_tail_idx) {
    // Buffer is full, move tail index forward
    drv->bat_sampling_buf_tail_idx++;
    if (drv->bat_sampling_buf_tail_idx >= PM_BATTERY_SAMPLING_BUF_SIZE) {
      drv->bat_sampling_buf_tail_idx = 0;
    }
  }
}

static void pm_parse_power_source_state(pm_driver_t* drv) {
  // Check USB power source status
  if (drv->pmic_data.usb_status != 0x0) {
    if (!drv->usb_connected) {
      drv->usb_connected = true;
    }
  } else {
    if (drv->usb_connected) {
      drv->usb_connected = false;
    }
  }

  // Check wireless charger status
  if (drv->wireless_data.vout_ready) {
    if (!drv->wireless_connected) {
      drv->wireless_connected = true;
    }
  } else {
    if (drv->wireless_connected) {
      drv->wireless_connected = false;
    }
  }

  // Check battery voltage for critical (undervoltage) threshold
  if ((drv->pmic_data.vbat < PM_BATTERY_UNDERVOLT_THR_V) &&
      !drv->battery_critical) {
    // Force Fuel gauge to 0, keep the covariance
    fuel_gauge_set_soc(&drv->fuel_gauge, 0.0f, drv->fuel_gauge.P);

    drv->battery_critical = true;
  } else if (drv->pmic_data.vbat > (PM_BATTERY_UNDERVOLT_RECOVERY_THR_V) &&
             drv->battery_critical) {
    drv->battery_critical = false;
  }
}

void pm_battery_initial_soc_guess(void) {
  pm_driver_t* drv = &g_pm;

  irq_key_t irq_key = irq_lock();

  // Check if the buffer is full
  if (drv->bat_sampling_buf_head_idx == drv->bat_sampling_buf_tail_idx) {
    // Buffer is empty, no data to process
    return;
  }

  // Calculate average voltage, current and temperature from the sampling
  // buffer and run the fuel gauge initial guess
  uint8_t buf_idx = drv->bat_sampling_buf_tail_idx;
  uint8_t samples_count = 0;
  float vbat_g = 0.0f;
  float ibat_g = 0.0f;
  float ntc_temp_g = 0.0f;
  while (drv->bat_sampling_buf_head_idx != buf_idx) {
    vbat_g += drv->bat_sampling_buf[buf_idx].vbat;
    ibat_g += drv->bat_sampling_buf[buf_idx].ibat;
    ntc_temp_g += drv->bat_sampling_buf[buf_idx].ntc_temp;

    buf_idx++;
    if (buf_idx >= PM_BATTERY_SAMPLING_BUF_SIZE) {
      buf_idx = 0;
    }

    samples_count++;
  }

  // Calculate average values
  vbat_g /= samples_count;
  ibat_g /= samples_count;
  ntc_temp_g /= samples_count;

  fuel_gauge_initial_guess(&drv->fuel_gauge, vbat_g, ibat_g, ntc_temp_g);

  irq_unlock(irq_key);
}

#endif
