#!/bin/bash
# vtium draws color emoji from Noto Color Emoji beside the text, two cells
# wide, with the sequences that join several emoji into one picture: people
# and families joined by zero-width joiners, flags, keycaps and skin tones.
# alt: A console screen with lines of text mixing words and color emoji,
# alt: then rows of faces, animals, food and objects, people and families
# alt: drawn as single pictures, national flags, number keycaps, and hands
# alt: in five skin tones.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
z=$'\xe2\x80\x8d' v=$'\xef\xb8\x8f' k=$'\xe2\x83\xa3'
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
row() { printf '  %s%-11s%s %s\n\n' "$d" "$1" "$o" "$2"; }
printf '\n  %sEmoji, in color%s\n\n' "$h" "$o"
printf '  Coffee ☕ first, then a walk 🚶 in the park 🌳. Rain 🌧️ by noon, sun 🌤️ by tea 🍵.\n'
printf '  The build passed 🎉, the tests are green 🟢, and the release ships 🚀 tonight 🌙.\n\n'
row Faces '😀 😃 😄 😁 😆 😅 🤣 😂 😋 😉 😊 😇 🥰 😍 🤩 😘 😎 🤓 🧐 🤔 🤗 😴 🥳 😮'
row Nature '🐶 🐱 🦊 🐻 🐼 🐨 🐯 🦁 🐮 🐸 🐵 🐧 🐦 🦉 🐢 🐙 🦋 🌵 🌻 🍁 🍄 🌈 🌊 🔥'
row Food '🍎 🍊 🍋 🍌 🍉 🍇 🍓 🫐 🍒 🥝 🍅 🥑 🥕 🌽 🥐 🧀 🍕 🍜 🍣 🍩 🍪 🎂 🍿 🧁'
row Things '🔭 📱 💻 🧮 🖥️ 🔋 💡 🔦 📷 🎧 🎸 🎮 🧭 🧲 🔧 🔩 🪛 🧪 📦 📚 🖊️ 📎 🔑 🎈'
row People "👩$z💻  👨$z🔬  👩$z🚀  🧑$z🍳  👨$z🎨  🧑$z🌾  👩$z🏫  🧑$z🚒  🕵️$z♀️  🧙$z♂️"
row Families "👨$z👩$z👧$z👦  👩$z👩$z👦  👨$z👨$z👧  👩$z👧  🧑$z🤝$z🧑  🏳️$z🌈  🐻$z❄️  🐦$z🔥"
row Flags '🇳🇴 🇯🇵 🇧🇷 🇮🇳 🇰🇪 🇨🇦 🇩🇪 🇰🇷 🇲🇽 🇳🇿 🇪🇸 🇺🇦 🇸🇪 🇿🇦 🇫🇷 🇮🇹 🇬🇧 🇺🇸 🇦🇷 🇪🇺 🇮🇸 🇻🇳'
row Keycaps "0$v$k 1$v$k 2$v$k 3$v$k 4$v$k 5$v$k 6$v$k 7$v$k 8$v$k 9$v$k #$v$k *$v$k 🔟"
row 'Skin tones' '👍 👍🏻 👍🏼 👍🏽 👍🏾 👍🏿   👋 👋🏻 👋🏼 👋🏽 👋🏾 👋🏿   🙌 🙌🏻 🙌🏼 🙌🏽 🙌🏾 🙌🏿'
