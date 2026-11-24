#!/bin/bash
# Creates macos/.venv with the kit's Python dependencies (pyusb, pycryptodomex, libusb-package).
# Needs Python 3.9 or later. No Homebrew needed: libusb comes from libusb-package.
cd "$(dirname "$0")" || exit 1
close() { echo; read -r -p "Press Return to close. "; exit "$1"; }
if ! python3 -c 'import sys; sys.exit(sys.version_info < (3, 9))' 2>/dev/null; then
  echo "Python 3.9 or later is required. Install it from python.org or the Xcode"
  echo "Command Line Tools (run: xcode-select --install) and try again."
  close 1
fi
python3 -m venv .venv || close 1
.venv/bin/python -m pip install --upgrade pip || close 1
.venv/bin/python -m pip install -r requirements.txt || close 1
echo
echo "Done. Setup complete."
close 0
