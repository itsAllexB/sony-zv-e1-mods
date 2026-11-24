#!/bin/bash
# ZV-E1 (firmware 2.02): reports whether the noise-reduction mod is currently
# applied. Read-only.
cd "$(dirname "$0")" || exit 1
exec "$PWD/run.sh" apply 76check
