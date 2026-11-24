#!/bin/bash
# ZV-E1 (firmware 2.02): read-only check that the camera is reachable over
# service mode. Nothing is changed.
cd "$(dirname "$0")" || exit 1
exec "$PWD/run.sh" check
