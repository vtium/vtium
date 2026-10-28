# The display's DRM atomic state, which tier 3's pointer case reads after
# each move of the pointer for where the cursor plane shows it
# (t3_pointer.py). It is printed on the event port between T3-PLANE-BEGIN and
# T3-PLANE-END, and each plane's lines on the screen. The pointer case runs
# it; it is no screen of the scenario's.
# screen-text: none, it prints the guest's DRM state.
out=${T3_EVENTS:-/dev/stdout}
state=
for f in /sys/kernel/debug/dri/*/state; do
    [ -r "$f" ] && { state=$f; break; }
done
if [ -z "$state" ]; then
    echo "T3-PLANE-ERROR no readable /sys/kernel/debug/dri/*/state" > "$out"
    exit 0
fi
{
    echo T3-PLANE-BEGIN
    cat "$state"
    echo T3-PLANE-END
} > "$out"
awk '/^plane\[/ { show = 1 } /^[^\t]/ && !/^plane\[/ { show = 0 }
     show && !/^\t\t/' "$state"
