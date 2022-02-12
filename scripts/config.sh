#!/bin/bash
# collator executable name
CLIENT_COLLATOR=parachain-collator-client
SERVICE_COLLATOR=parachain-collator-service
# relay executable name
RELAY_NODE=polkadot
# polkadot version
RELAY_VER="0.9.15"
# rust toolchain version
RUST_TOOLCHAIN=nightly-2021-11-11
# rococo (polkadot) source code revision
POLKADOT_COMMIT=4d94aea03300b85ddbfaa5b28e1078545e0545a2
# node key of the first relay node
NODE_KEY=b18f8a0ee1a4ac13d90c3d3ca4b813d2bf070385415ca97d3a1532e967550b47
# p2p identifier of the first relay node
BOOT_NODE=12D3KooWDDD72cGrLUoF89ndKsmaHmWLSJXMC8p3a5ibh1J1VBKd

# set logger output
#*_LOGCFG=info,rpc=trace,sycollator=trace,collation=trace,sync=trace,parachain=trace
RELAY_LOGCFG=warn
CLIENT_LOGCFG=info
SERVICE_LOGCFG=info

# 1 - save previous collator build in *.bak
BACKUP=1
# 1 - use persistent storage for node databases
PERSISTENT=0

red=`tput setaf 1`
green=`tput setaf 2`
blue=`tput setaf 5`
blue2=`tput setaf 6`
reset=`tput sgr0`

# exclusive
LOCKFD=99
LOCKFILE="/var/lock/`basename $0`"

function _lock()             { flock -$1 $LOCKFD; }
function _no_more_locking()  { _lock u; _lock xn && rm -f $LOCKFILE; }
function _prepare_locking()  { eval "exec $LOCKFD>\"$LOCKFILE\""; trap _no_more_locking EXIT; }