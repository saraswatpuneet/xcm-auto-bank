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

# Parachain addresses

#  para100  = '5Ec4AhP7HwJNrY2CxEcFSy1BuqAY3qxvCQCfoois983TTxDA'
#  para200  = '5Ec4AhPTL6nWnUnw58QzjJvFd3QATwHA3UJnvSD4GVSQ7Gop'
#  para1000 = '5Ec4AhPZk8STuex8Wsi9TwDtJQxKqzPJRCH7348Xtcs9vZLJ'

# Sibling addresses

#  sibl100  = '5Eg2fnsvNfVGz8kMWEZLZcM1AJqqmG22G3r74mFN1r52Ka7S'
#  sibl200  = '5Eg2fntGQpyQv5X5d8N5qxG4sX5UBMLG77xEBPjZ9DTxxtt7'

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
        call_module='ClientModule',
        call_function='register',
        call_params={
            'paraid': 200,
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
        module='ClientModule',
        storage_function='Device',
        params=[dev.ss58_address]
    )

    print(f"device {dev.ss58_address} {dev_profile} ")

    para100 = get_para_address(app, 100)
    para200 = get_para_address(app, 200)

    sibl100 = get_para_address(app, 100, prefix=b'sibl')
    sibl200 = get_para_address(app, 200, prefix=b'sibl')

    for (para,name) in [
        (para100,             'para 100'),
        (para200,             'para 200'),
        (sibl100,             'sibl 100'),
        (sibl200,             'sibl 200'),
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

if __name__=="__main__":
    import argparse
    parser = argparse.ArgumentParser(description='substrate command line.')
    parser.add_argument('command', help='endow register, account_info configure order complete')
    parser.add_argument('--ws_url', help='websocker url', nargs='*', default=['ws://localhost:9950/'] )
    parser.add_argument('--amount', help='tokens amount', type=int, default=10_000_000_000_000)
    parser.add_argument('--paraid', help='parachain id', nargs='*', type=int, default=[100] )
    parser.add_argument('--account', help='account uri (i.e  //Bob)', type=str, default='//Bob' )
    parser.add_argument('--dev', help='device name', type=str, default='//Device//1' )
    parser.add_argument('--wasm', help='wasm file')
    parser.add_argument('--genesis', help='genesis file')

    args = parser.parse_args()

    url = args.ws_url[0]
    print(f"connect to node by {url}")
    cmd = args.command
    dev = Keypair.create_from_uri( args.dev )

    substrate = SubstrateInterface(
       url=url,
       ss58_format=42,
       type_registry_preset='rococo',
       type_registry={'types': CUSTOM_TYPES }
    )
    substrate.update_type_registry_presets()

    if cmd=="endow":
        # (substrate instance ,paraid , account to transfer)
        # endow 1000 Unit for each parachain account in relay chain
        endow(substrate, get_para_address(substrate,100), args.amount)
        endow(substrate, get_para_address(substrate,200), args.amount)

    elif cmd=="endow_para":

        endow(substrate, get_para_address(substrate, args.paraid[0], prefix=b'sibl'), args.amount )

    elif cmd=="register":
        # (substrate instance ,paraid , wasm , genesis)
        register(substrate, args.paraid[0], args.wasm , args.genesis )

    elif cmd=="hrmp_open":
        # (substrate instance ,paraid from , paraid to)
        if len(args.paraid) != 2:
            sys.exit(1)

        hrmp_open(substrate, args.paraid[0], args.paraid[1] )

    elif cmd=="ump":
        if len(sys.argv) == 4:
            msg = open(sys.argv[3]).read()
        else:
            msg = '0x04000090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe220b00f0ab75a40d'
        ump(substrate, msg)

    elif cmd=="hrmp":
        if len(sys.argv) == 5:
            msg = open(sys.argv[4]).read()
        else:
            msg = '0x02000090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe220b0060b7986c88'


        hrmp(substrate, args.paraid[0], msg )

    elif cmd=="account_info":
        account_info(substrate)

    elif cmd=="show_call":
        show_call(substrate, args.amount )

    elif cmd=="configure":
        config_device(substrate, args.amount )
        if len(args.ws_url)>1:
            url = args.ws_url[1]
            substrate = SubstrateInterface(
               url=url,
               ss58_format=42,
               type_registry_preset='rococo',
               type_registry={'types': CUSTOM_TYPES }
            )
            config_device_srv(substrate, args.amount )

    elif cmd=="order":
        order(substrate, Keypair.create_from_uri(args.account),  args.amount )

    elif cmd=="order_reject":
        reject( substrate )

    elif cmd=="order_done":
        done( substrate )

    elif cmd=="configure__":
        config_device_srv(substrate, args.amount )

    else:
        print(f"unknown command '{cmd}'")
        print_usage()
