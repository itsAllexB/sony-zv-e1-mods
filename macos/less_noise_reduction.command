#!/bin/bash
# ZV-E1 (firmware 2.02): skips one temporal noise-reduction stage. In camera
# memory only; a battery pull (10 s) or undo_less_noise_reduction.command
# reverts it. No firmware or saved-setting writes.
cd "$(dirname "$0")" || exit 1
exec "$PWD/run.sh" apply 76
