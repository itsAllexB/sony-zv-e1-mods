#!/bin/sh
# Rebuilds the two embedded font subsets in assets/fonts/ (needs fonttools: pip install fonttools).
#   $1  InterVariable.ttf (Inter 4.x, https://github.com/rsms/inter, OFL-1.1)
#   $2  lucide.ttf        (Lucide icon font, https://lucide.dev, ISC)
# Add a codepoint to ICONS when the UI starts using a new Lucide icon (see `mod icon` in src/gui.rs).
set -eu
cd "$(dirname "$0")/.."
INTER=$1
LUCIDE=$2

# Latin, Latin-1, Latin Extended-A/B, Greek, Cyrillic, Latin Extended Additional, punctuation, arrows, ×, −.
TEXT="U+0020-007E,U+00A0-024F,U+0370-03FF,U+0400-04FF,U+1E00-1EFF,U+2000-206F,U+20AC,U+2122,U+2190-2199,U+2212"
pyftsubset "$INTER" --unicodes="$TEXT" \
    --layout-features="kern,liga,calt,ccmp,locl,mark,mkmk,case" \
    --output-file=assets/fonts/Inter-subset.ttf

#        settings check folder-open plus folder-down trash-2 info shield-check
ICONS="U+E154,U+E06C,U+E247,U+E13D,U+E332,U+E18E,U+E0F9,U+E1FF"
#        circle-check circle-minus circle-x circle-alert clock arrow-down-to-line list-x
ICONS="$ICONS,U+E226,U+E07E,U+E084,U+E077,U+E087,U+E455,U+E240"
#        minus square copy x folder
ICONS="$ICONS,U+E11C,U+E167,U+E09E,U+E1B2,U+E0D7"
pyftsubset "$LUCIDE" --unicodes="$ICONS" --layout-features="" --drop-tables+=GSUB,GPOS \
    --output-file=assets/fonts/lucide-subset.ttf
ls -l assets/fonts/*.ttf
