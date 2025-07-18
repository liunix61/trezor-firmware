from typing import TYPE_CHECKING

from . import seed

if TYPE_CHECKING:
    from trezor.messages import CardanoAddress, CardanoGetAddress

    from apps.common.keychain import Keychain as Slip21Keychain


@seed.with_keychain
async def get_address(
    msg: CardanoGetAddress, keychain: seed.Keychain, slip21_keychain: Slip21Keychain
) -> CardanoAddress:
    from trezor import log, wire
    from trezor.messages import CardanoAddress

    from apps.common.address_mac import get_address_mac

    from . import addresses
    from .helpers.credential import Credential, should_show_credentials
    from .helpers.paths import SLIP44_ID
    from .helpers.utils import validate_network_info
    from .layout import show_cardano_address, show_credentials

    address_parameters = msg.address_parameters  # local_cache_attribute

    validate_network_info(msg.network_id, msg.protocol_magic)
    addresses.validate_address_parameters(address_parameters)

    try:
        address = addresses.derive_human_readable(
            keychain, address_parameters, msg.protocol_magic, msg.network_id
        )
    except ValueError as e:
        if __debug__:
            log.exception(__name__, e)
        raise wire.ProcessError("Deriving address failed")

    mac = get_address_mac(
        address, SLIP44_ID, address_parameters.address_n, slip21_keychain
    )
    if msg.show_display:
        # _display_address
        if should_show_credentials(address_parameters):
            await show_credentials(
                Credential.payment_credential(address_parameters),
                Credential.stake_credential(address_parameters),
            )
        await show_cardano_address(
            address_parameters, address, msg.protocol_magic, chunkify=bool(msg.chunkify)
        )

    return CardanoAddress(address=address, mac=mac)
