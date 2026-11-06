"""Export the filled SVG geometry to PNG and multiresolution ICO with Pillow."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageChops

root = Path(__file__).resolve().parents[1]
scale = 4
size = (512 * scale, 512 * scale)


def rounded(draw, box, radius, fill):
    draw.rounded_rectangle(tuple(v * scale for v in box), radius=radius * scale, fill=fill)


def mouse(box, wheel, color):
    layer = Image.new('RGBA', size)
    rounded(ImageDraw.Draw(layer), box, 112, color)
    rounded(ImageDraw.Draw(layer), wheel, 8, (0, 0, 0, 0))
    return layer


rear = mouse((75, 45, 299, 361), (179, 75, 195, 139), '#929292')
mask = Image.new('L', size, 255)
rounded(ImageDraw.Draw(mask), (197, 135, 445, 475), 124, 0)
rear.putalpha(ImageChops.multiply(rear.getchannel('A'), mask))
rear.alpha_composite(mouse((209, 147, 433, 463), (313, 177, 329, 241), '#242424'))
image = rear.resize((512, 512), Image.Resampling.LANCZOS)
image.save(root / 'assets/app-icon-filled.png')
image.save(root / 'assets/app-icon-filled.ico', sizes=[(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
print('Exported filled PNG and nine ICO sizes')
