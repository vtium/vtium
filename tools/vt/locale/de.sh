# shellcheck shell=sh
# German mode, one of vtium's locale mode scripts. Source it in a vtium
# console, as in `. de.sh`, to switch that console and its shell to German.
#
# The script switches the console's font set to terminus, the Terminus bitmap
# font that every vtium build installs. Its Latin letters include German's
# umlauts and ß. The switch goes through `command vtium-config use-frit`.
# `command` skips any shell function or alias named vtium-config, so the real
# program decides the switch. vtium-config sends OSC 50 and checks that vtium
# took the font set. Then the script sets LANG to de_DE.UTF-8 in the shell
# that sources it. A program cannot change the environment of the shell that
# started it, so the script is sourced rather than run. When vtium does not
# take the font set, the script leaves LANG as it was, prints a line that says
# so, and returns 1. In a shell with set -e, which exits when a command fails,
# the script returns 0 instead, so that the failure leaves the shell running.
# The script sets no other variable and changes nothing else in the shell.
#
# vtium sets the keyboard layout when it starts. While it runs, only a change
# to the whole system's layout through localed changes it, in a build with
# logind. So the script names the German layout and says how to set it.
#
# References:
# - ADR-1511 D3: what a locale mode script switches, and that it returns.

if command vtium-config use-frit terminus; then
	export LANG=de_DE.UTF-8
	echo 'German mode: LANG is de_DE.UTF-8, and the font set is terminus.'
	echo 'The keyboard layout stays as it was, because vtium sets the layout when it starts.'
	echo 'For the German layout, set xkb-layout = de in vtium.conf and start vtium again.'
	[ -z "${LC_ALL:-}" ] || echo 'LC_ALL is set, and it overrides LANG. Unset LC_ALL to use the German locale.'
else
	echo 'German mode failed: vtium did not switch to the font set terminus, so LANG stays as it was.' >&2
	# A shell with set -e would exit on a status of 1, so the script returns 0 there.
	case $- in *e*) return 0 ;; esac
	return 1
fi
