#!/bin/bash
# ZV-E1 (firmware 2.02): puts the noise-reduction mod back to stock without a
# battery pull. In camera memory only; no firmware or saved-setting writes.
cd "$(dirname "$0")" || exit 1
exec "$PWD/run.sh" apply 76r
