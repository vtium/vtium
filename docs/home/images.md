# Images

Programs show images in vtium through sixel, the kitty graphics protocol, and iTerm2's inline images. vtium plays the animations of the kitty graphics protocol, and draws the protocol's Unicode placeholders. Each image is anchored to a line of text, so the image scrolls with the text.

A separate program, the image helper, reads and decodes every image. The image helper runs as your user, never as root, inside a sandbox.

See [Images](../features.md#images), in What vtium does, for the builds that show images and for each protocol's details.
