#!/usr/bin/env python3
import sys
from substrateinterface import SubstrateInterface, Keypair
from substrateinterface.exceptions import SubstrateRequestException
from scalecodec import ScaleBytes, ScaleType

# Relay chain root account
root = Keypair.create_from_uri('//Alice')
# Regular relay chain account
bob = Keypair.create_from_uri('//Bob')

charlie = Keypair.create_from_uri('//Charlie')
dev = Keypair.create_from_uri('//Device//1')


CUSTOM_TYPES = {
    "DeviceState": {
        "type": "enum",
        "value_list": [
            "Off",
            "Ready",
            "Busy",
            "Accepted",
            "Timewait"
        ]
    },
    "DeviceProfile": {
        "type": "struct",
        "type_mapping": [
            ["state", "DeviceState"],
            ["penalty", "Balance"],
            ["wcd", "Moment"],
            ["paraid", "u32"]
        ]
    },
    "OrderOf": {
        "type": "struct",
        "type_mapping": [
            ["until", "Moment"],
            ["args", "u64"],
            ["fee", "Balance"],
            ["client", "AccountId"],
            ["paraid", "u32"],
        ]
    },
    "OrderBaseOf": {
        "type": "struct",
        "type_mapping": [
            ["until", "Moment"],
            ["args", "u64"],
            ["fee", "Balance"],
            ["device", "AccountId"],
        ]
    }
}

def get_para_address(app, paraid, prefix=b'para'):
    '''
    Returns parachain address in ss58 format

    :param app  substrate connection instance
    :param  paraid parachain id
    :prefix  'para' - parachain address in parent (relay) chain
             'sibl' - parachain address in other (sibling) chain

    parachain address consists of b'para' + encoded(parachain id ) + 00...00 up to 32 bytes
    '''

    addr = bytearray( prefix )
    addr.append( paraid & 0xFF )
    paraid = paraid>>8
    addr.append( paraid & 0xFF )
    paraid = paraid>>8
    addr.append( paraid & 0xFF )

    return app.ss58_encode( addr.ljust(32,b'\0') )

def endow(app, dest, amount):
    '''
    Transfer tokens to parachain account in relay chain

    :param app  substrate connection instance
    :param dest parachain address in ss58 format
    :param amount the number of tokens to transfer
    '''
    # compose `Balance.transfer`
    call = app.compose_call(
        call_module='Balances',
        call_function='transfer',
        call_params={
            'dest': dest,
            'value': amount
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=root)
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def register(app, paraid, wasm_file, genesis_file):
    '''
    Register parachain in relay chain

    :param app  substrate connection instance
    :param paraid parachain id
    :param wasm_file  path to the file with parachain runtime
    :param genesis_file path to the file with parachain genesis state
    '''

    wasm = open(wasm_file).read()
    genesis = open(genesis_file).read()
    # Register parachains
    payload = app.compose_call(
        call_module='ParasSudoWrapper',
        call_function='sudo_schedule_para_initialize',
        call_params={
            'id': paraid,
            'genesis': {
                'genesisHead': genesis,
                'validationCode': wasm,
                'parachain': True
            }
        }
    )
    call = app.compose_call(
        call_module='Sudo',
        call_function='sudo',
        call_params={
            'call': payload.value,
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=root)
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def hrmp_open(app, pfrom, pto):
    '''
    Open unidirectional HRMP channel between 100 and 200 parachains.

    :param app  substrate connection instance

    '''
    assert pfrom!=pto

    # establish HRMP channel between 100 and 200 parachains
    payload = app.compose_call(
        call_module='ParasSudoWrapper',
        call_function='sudo_establish_hrmp_channel',
        call_params={
            'sender': pfrom,
            'recipient': pto,
            'max_capacity': 5,
            'max_message_size': 500,
        }
    )



    call = app.compose_call(
        call_module='Sudo',
        call_function='sudo',
        call_params={
            'call': payload.value,
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=root)
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)


def ump(app, msg):
    '''
    Transfer 15 tokens to Charlie in relay chain by passing Ump message into 100 parachain

    :param app  substrate connection instance
    '''
    call = app.compose_call(
        call_module='TemplateModule',
        call_function='send_relay_chain',
        call_params={
            'call': msg
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=bob )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def hrmp(app, paraid, msg):
    '''
    Transfer 15 tokens to Charlie in 200 parachain by passing xmp (hrmp) message via 100 parachain

    :param app  substrate connection instance
    '''
    call = app.compose_call(
        call_module='TemplateModule',
        call_function='send_para_chain',
        call_params={
            'paraid': paraid,
            'call': msg
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=bob )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def show_call(app, amount):
    '''
    Display hex encoded Balance.transfer call

    '''
    call = app.compose_call(
        call_module='Balances',
        call_function='transfer',
        call_params={
            'dest': charlie.ss58_address,
            'value': amount
        }
    )
    print(call.encode().to_hex())

def done(app):
    call = app.compose_call(
        call_module='ServiceModule',
        call_function='done',
        call_params={
            'onoff': True
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=dev )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def reject(app):
    call = app.compose_call(
        call_module='ServiceModule',
        call_function='accept',
        call_params={
            'reject': True,
            'onoff': True
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=dev )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

def order(app, account, amount):
    '''
    Order
    '''
    # get timestamp
    now = app.query(
        module='Timestamp',
        storage_function='Now',
        params=[]
    )

    call = app.compose_call(
        call_module='ClientModule',
        call_function='order',
        call_params={
            'order': {
                'until': (now.value + 10000000) ,
                'args': 0,
                'fee': 200_000_000_000,
                'device': dev.ss58_address,
            }
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=account )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)


def config_device_srv(app, amount):
    '''
    Endow device account,
    Register devices in client and service side
    '''
    call = app.compose_call(
        call_module='Balances',
        call_function='transfer',
        call_params={
            'dest': dev.ss58_address,
            'value': amount
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=bob )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)
    print("transfer")

    '''
    Register devices in client and service side
    '''
    call = app.compose_call(
        call_module='ServiceModule',
        call_function='register',
        call_params={
            'penalty': 1000_000_000,
            'wcd': 3600000,
            'onoff': True,
        }
    )

    extrinsic = app.create_signed_extrinsic(call=call, keypair=dev )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)


def config_device(app, amount):
    '''
    Endow device account,
    Register devices in client and service side
    '''
    call = app.compose_call(
        call_module='Balances',
        call_function='transfer',
        call_params={
            'dest': dev.ss58_address,
            'value': amount
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=bob )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)
    print("transfer")

    call = app.compose_call(
        call_module='XchangePallet',
        call_function='register',
        call_params={
            'paraid': 2000,
            'penalty': 1000_000_000,
            'wcd': 3600000,
            'onoff': True
        }
    )
    extrinsic = app.create_signed_extrinsic(call=call, keypair=dev )
    receipt = app.submit_extrinsic(extrinsic, wait_for_inclusion=True)

    print( f"configured device '{dev}' with address {dev.ss58_address} " )

def account_info(app):
    '''
    Display typical account balances
    '''
    dev_profile = app.query(
        module='XchangePallet',
        storage_function='Device',
        params=[dev.ss58_address]
    )

    print(f"device {dev.ss58_address} {dev_profile} ")

    para100 = get_para_address(app, 2000)
    para200 = get_para_address(app, 2001)

    sibl100 = get_para_address(app, 2000, prefix=b'sibl')
    sibl200 = get_para_address(app, 2001, prefix=b'sibl')

    for (para,name) in [
        (para100,             'para 2000'),
        (para200,             'para 2001'),
        (sibl100,             'sibl 2000'),
        (sibl200,             'sibl 2001'),
        (dev.ss58_address,    'Device'),
        (root.ss58_address,   'Alice'),
        (bob.ss58_address,    'Bob'),
        (charlie.ss58_address,'Charlie')]:
        result = substrate.query(
            module='System',
            storage_function='Account',
            params=[para]
        )
        if result is None:
            print(f"'{name}' ({para}) is gone")
            continue
        print(f"'{name}' ({para}) balance {result.value['data']['free']} ")


substrate = SubstrateInterface(
   url="ws://localhost:9944/",
   ss58_format=42,
   type_registry_preset='rococo',
   type_registry={'types': CUSTOM_TYPES }
)
substrate.update_type_registry_presets()


config_device(substrate, 100_000_000_000)