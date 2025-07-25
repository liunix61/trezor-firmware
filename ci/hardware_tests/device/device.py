import datetime
import sys
import time
from subprocess import run


class Device:
    def __init__(self, *, uhub_location=None, hub_vendor=None, device_port=None):
        self.uhub_location = uhub_location
        self.hub_vendor = hub_vendor
        self.device_port = device_port

    @staticmethod
    def log(msg):
        print(msg, flush=True, file=sys.stderr)

    def run_trezorctl(self, cmd: str, **kwargs):
        full_cmd = "trezorctl "
        full_cmd += cmd
        self.log(f"[software/trezorctl] Running '{full_cmd}'")
        return run(full_cmd, shell=True, check=True, **kwargs)

    def check_model(self, model=None):
        res = self.run_trezorctl("list", capture_output=True, text=True)
        self.log(res.stdout)
        self.log(res.stderr)
        self.run_trezorctl("get-features | grep version")
        lines = res.stdout.splitlines()
        if len(lines) != 1:
            raise RuntimeError(f"{len(lines)} Trezors connected")
        if model and model not in lines[0]:
            raise RuntimeError(f"invalid Trezor model connected (expected {model})")
        return lines[0].split()[0]

    def reboot(self):
        self.power_off()
        self.power_on()

    def _hub(self):
        if self.hub_vendor:
            return f"--vendor {self.hub_vendor}"
        else:
            return f"-l {self.uhub_location}"

    def power_on(self):
        self.now()
        self.log("[hardware/usb] Turning power on...")
        run(
            f"uhubctl {self._hub()} -p {self.device_port} -a on",
            shell=True,
            check=True,
        )
        self.wait(3)

    def power_off(self):
        self.now()
        self.log("[hardware/usb] Turning power off...")
        run(
            f"uhubctl {self._hub()} -p {self.device_port} -r 5 -a off",
            shell=True,
            check=True,
        )
        self.wait(3)

    def touch(self, location, action):
        raise NotImplementedError

    @staticmethod
    def wait(seconds):
        Device.now()
        Device.log(f"[software] Waiting for {seconds} seconds...")
        time.sleep(seconds)

    @staticmethod
    def now():
        Device.log(f"\n[timestamp] {datetime.datetime.now()}")
