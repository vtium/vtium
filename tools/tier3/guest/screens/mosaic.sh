#!/bin/bash
# look-for: Braille as square dots on one 2x4 grid per cell; sextants, octants
# look-for: as solid blocks; no seams. The octant band is toothed, with a row of
# look-for: square holes: that is its shape.
# wrong-looks-like: Dots or blocks off the cell grid, a dark seam between rows,
# wrong-looks-like: or empty boxes: stock kmscon takes these from the font.
# Every Braille pattern, sextant and octant in codepoint order, then figures
# stacked across cells (checkerboards of sextants and octants, a grid of
# eight-dot Braille, a Braille wave, sextant and octant bands across the row
# boundaries), then the checkerboards and grid in yellow on blue, as tier 2's
# mosaic screen: each cell is drawn to the cell's full size (#76). No octant
# fills a plain band (OCTANT-12 and -78 would be the quarter blocks outside
# the range), so the octant band, OCTANT-678 over 123678 over 123, has teeth
# and a row of square holes.
printf 'braille ⠀⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟⠠⠡⠢⠣⠤⠥⠦⠧⠨⠩⠪⠫⠬⠭⠮⠯⠰⠱⠲⠳⠴⠵⠶⠷⠸⠹⠺⠻⠼⠽⠾⠿\n'
printf '        ⡀⡁⡂⡃⡄⡅⡆⡇⡈⡉⡊⡋⡌⡍⡎⡏⡐⡑⡒⡓⡔⡕⡖⡗⡘⡙⡚⡛⡜⡝⡞⡟⡠⡡⡢⡣⡤⡥⡦⡧⡨⡩⡪⡫⡬⡭⡮⡯⡰⡱⡲⡳⡴⡵⡶⡷⡸⡹⡺⡻⡼⡽⡾⡿\n'
printf '        ⢀⢁⢂⢃⢄⢅⢆⢇⢈⢉⢊⢋⢌⢍⢎⢏⢐⢑⢒⢓⢔⢕⢖⢗⢘⢙⢚⢛⢜⢝⢞⢟⢠⢡⢢⢣⢤⢥⢦⢧⢨⢩⢪⢫⢬⢭⢮⢯⢰⢱⢲⢳⢴⢵⢶⢷⢸⢹⢺⢻⢼⢽⢾⢿\n'
printf '        ⣀⣁⣂⣃⣄⣅⣆⣇⣈⣉⣊⣋⣌⣍⣎⣏⣐⣑⣒⣓⣔⣕⣖⣗⣘⣙⣚⣛⣜⣝⣞⣟⣠⣡⣢⣣⣤⣥⣦⣧⣨⣩⣪⣫⣬⣭⣮⣯⣰⣱⣲⣳⣴⣵⣶⣷⣸⣹⣺⣻⣼⣽⣾⣿\n'
printf 'sextants🬀🬁🬂🬃🬄🬅🬆🬇🬈🬉🬊🬋🬌🬍🬎🬏🬐🬑🬒🬓🬔🬕🬖🬗🬘🬙🬚🬛🬜🬝🬞🬟🬠🬡🬢🬣🬤🬥🬦🬧🬨🬩🬪🬫🬬🬭🬮🬯🬰🬱🬲🬳🬴🬵🬶🬷🬸🬹🬺🬻\n'
printf 'octants 𜴀𜴁𜴂𜴃𜴄𜴅𜴆𜴇𜴈𜴉𜴊𜴋𜴌𜴍𜴎𜴏𜴐𜴑𜴒𜴓𜴔𜴕𜴖𜴗𜴘𜴙𜴚𜴛𜴜𜴝𜴞𜴟𜴠𜴡𜴢𜴣𜴤𜴥𜴦𜴧𜴨𜴩𜴪𜴫𜴬𜴭𜴮𜴯𜴰𜴱𜴲𜴳𜴴𜴵𜴶𜴷𜴸𜴹𜴺𜴻𜴼𜴽𜴾𜴿\n'
printf '        𜵀𜵁𜵂𜵃𜵄𜵅𜵆𜵇𜵈𜵉𜵊𜵋𜵌𜵍𜵎𜵏𜵐𜵑𜵒𜵓𜵔𜵕𜵖𜵗𜵘𜵙𜵚𜵛𜵜𜵝𜵞𜵟𜵠𜵡𜵢𜵣𜵤𜵥𜵦𜵧𜵨𜵩𜵪𜵫𜵬𜵭𜵮𜵯𜵰𜵱𜵲𜵳𜵴𜵵𜵶𜵷𜵸𜵹𜵺𜵻𜵼𜵽𜵾𜵿\n'
printf '        𜶀𜶁𜶂𜶃𜶄𜶅𜶆𜶇𜶈𜶉𜶊𜶋𜶌𜶍𜶎𜶏𜶐𜶑𜶒𜶓𜶔𜶕𜶖𜶗𜶘𜶙𜶚𜶛𜶜𜶝𜶞𜶟𜶠𜶡𜶢𜶣𜶤𜶥𜶦𜶧𜶨𜶩𜶪𜶫𜶬𜶭𜶮𜶯𜶰𜶱𜶲𜶳𜶴𜶵𜶶𜶷𜶸𜶹𜶺𜶻𜶼𜶽𜶾𜶿\n'
printf '        𜷀𜷁𜷂𜷃𜷄𜷅𜷆𜷇𜷈𜷉𜷊𜷋𜷌𜷍𜷎𜷏𜷐𜷑𜷒𜷓𜷔𜷕𜷖𜷗𜷘𜷙𜷚𜷛𜷜𜷝𜷞𜷟𜷠𜷡𜷢𜷣𜷤𜷥\n'
printf '\n'
printf '🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠀⡠⠔⠉⠉⠑⠤⡀⠀⠀⠀⠀⠀⠀⠀⠀ 🬭🬭🬭🬭 𜷋𜷋𜷋𜷋\n'
printf '🬤🬤🬤🬤🬤🬤🬤🬤 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠜⠀⠀⠀⠀⠀⠀⠈⢢⠀⠀⠀⠀⠀⠀⢀ 🬰🬰🬰🬰 𜷒𜷒𜷒𜷒\n'
printf '🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠑⠢⣀⣀⡠⠒⠁ 🬂🬂🬂🬂 𜴂𜴂𜴂𜴂\n'
printf '\n'
printf '\033[33;44m🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿\n🬤🬤🬤🬤🬤🬤🬤🬤 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿\033[0m\n'
