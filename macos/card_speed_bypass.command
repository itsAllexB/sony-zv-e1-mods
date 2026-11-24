#!/bin/bash
# ZV-E1 (firmware 2.02): applies the card-speed bypass. In camera memory only;
# a battery pull reverts it. No firmware or saved-setting writes.
cd "$(dirname "$0")" || exit 1
exec "$PWD/run.sh" apply 70
