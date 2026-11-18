#!/bin/bash
# The probe row of font-probe.sh drawn with the reference set cullet-alone,
# which holds Cullet Mono Regular alone (t3_fonts.py). vtium-config use-frit
# switches the console to it for this login session, and the script reports
# what use-frit printed on the event port, so the case knows the switch took.
# The screen is cleared once the switch is done.
printf 'T3-USE-FRIT %s\n' "$(vtium-config use-frit cullet-alone 2>&1 | tr '\n' ' ')" \
    > "${T3_EVENTS:-/dev/null}"
clear
bash "$(dirname "$0")"/font-probe.sh
