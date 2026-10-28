#!/bin/sh
# The menu-bar hammer while lightning strikes (tray.rs, zap.rs): the idle
# glyph from hammer.png (rendered from hammer.svg) with a soft halo in five
# strengths; the idle icon itself is strength 0. Template images: black, the
# shape in alpha, 36 px = 18 pt at 2x like the idle icon, so nothing moves.
# Run once after changing the logo and commit the PNGs. Needs ffmpeg.
set -eu
cd "$(dirname "$0")"

# At 4x: blur, add the halo under the glyph, fade it out over the last
# 2 px of the canvas (the mark leaves only ~3 px above and below), shrink.
for i in 1 2 3 4 5; do
    gain=$(echo "scale=3; $i * 0.3" | bc)
    ffmpeg -v error -y -i hammer.png -filter_complex "
        [0]format=gray,scale=144:144:flags=area,split[g][b];
        [b]gblur=sigma=7[h];
        [g][h]blend=all_expr='A+min(150,B*$gain)*(min(1,min(min(X,W-1-X),min(Y,H-1-Y))/8))*(255-A)/255'[a];
        color=black:s=144x144,format=rgb24[k];
        [k][a]alphamerge,scale=36:36:flags=area,format=rgba" \
        -frames:v 1 "glow-$i.png"
done
ls -l glow-*.png
