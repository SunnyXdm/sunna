#!/usr/bin/env python3
"""Only derivatives are resized. Source PNGs are untouched display captures."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
root=Path(__file__).parent
files=sorted(root.glob('0*.png'), key=lambda p:(p.stem.removesuffix('-b'), p.stem.endswith('-b')))
font=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf',20)
sheet=Image.new('RGB',(1920,5*650),(22,25,30))
draw=ImageDraw.Draw(sheet)
for i,path in enumerate(files):
 im=Image.open(path).convert('RGB')
 im.resize((1600,round(im.height*1600/im.width)),Image.Resampling.LANCZOS).save(path.with_suffix('.jpg'),quality=85)
 tile=im.copy();tile.thumbnail((936,590),Image.Resampling.LANCZOS)
 x=(i%2)*960+12;y=(i//2)*650+46
 sheet.paste(tile,(x,y))
 draw.text((x,y-32),path.stem,fill=(232,235,240),font=font)
sheet.save(root/'contact.png')
