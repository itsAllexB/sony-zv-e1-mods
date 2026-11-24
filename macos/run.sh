#!/bin/bash
# Shared launcher plumbing for the kit's .command files.
# Usage: run.sh <subcommand> [args...]
# Passes the subcommand to zvemod.py under sudo (needed to detach macOS's
# mass-storage driver from the camera). Hands any files written under sudo
# back to the invoking user.
cd "$(dirname "$0")" || exit 1
PY="$PWD/.venv/bin/python"
close() { echo; read -r -p "Press Return to close. "; exit "$1"; }
if [ ! -x "$PY" ]; then
  echo "Python environment missing: run setup.command first."
  close 1
fi
echo "macOS asks for your password: taking the camera over from its own USB"
echo "driver needs admin rights."
sudo /bin/bash -c 'trap : INT; here=$1; shift; "$@"; status=$?
  [ -d "$here/results" ] && chown -R "$SUDO_UID:$SUDO_GID" "$here/results"
  exit $status' run "$PWD" "$PY" -I -B "$PWD/zvemod.py" "$@"
close $?
