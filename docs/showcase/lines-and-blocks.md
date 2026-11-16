<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Lines and blocks

These screens are part of [vtium's showcase](README.md), which says how they are made.

![A console dashboard of five framed panels with rounded and double borders: a bar chart of block elements, a sine wave plotted in Braille dots, disk gauges, a table of services with colored status marks, and a filled area chart drawn in sextants.](boxes.png)

vtium draws box drawing, block elements, Braille and sextants itself, edge to edge in every cell. So lines join and bars stack without gaps, whatever the font. Here a script draws a dashboard with them.

