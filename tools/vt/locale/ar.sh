# shellcheck shell=sh
# Arabic mode, one of vtium's locale mode scripts. Source it in a vtium
# console, as in `. ar.sh`, to switch that console and its shell to Arabic.
#
# The script switches the console's font set to noto-arabic, which draws the
# Arabic script with Noto Sans Arabic. Arabic is written right to left. The
# switch goes through `vtium-config use-frit`, which sends OSC 50 and checks
# that vtium took the font set. Then the script sets LANG to ar_EG.UTF-8 in
# the shell that sources it. A program cannot change the environment of the
# shell that started it, so the script is sourced rather than run. When vtium
# does not take the font set, the script leaves LANG as it was and returns 1.
# It sets no other variable and changes nothing else in the shell.
#
# vtium sets the keyboard layout when it starts. While it runs, only a change
# to the whole system's layout through localed changes it, in a build with
# logind. So the script names the Arabic layout and says how to set it.
#
# References:
# - ADR-1511 D3: what a locale mode script switches, and that it returns.

if vtium-config use-frit noto-arabic; then
	export LANG=ar_EG.UTF-8
	echo 'Arabic mode: LANG is ar_EG.UTF-8, and the font set is noto-arabic.'
	echo 'The keyboard layout stays as it was, because vtium sets the layout when it starts.'
	echo 'For the Arabic layout, set xkb-layout = ara in vtium.conf and start vtium again.'
	[ -z "${LC_ALL:-}" ] || echo 'LC_ALL is set, and it overrides LANG. Unset LC_ALL to use the Arabic locale.'
else
	echo 'Arabic mode failed: vtium did not switch to the font set noto-arabic, so LANG stays as it was.' >&2
	return 1
fi
