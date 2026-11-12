from PIL import Image, ImageDraw, ImageFont
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ASSETS = os.path.join(HERE, "assets")
os.makedirs(ASSETS, exist_ok=True)

S = 1024
# vertical gradient: friendly blue -> deeper blue
top = (56, 132, 255)
bot = (24, 78, 214)
line = Image.new("RGBA", (1, S))
for y in range(S):
    t = y / (S - 1)
    line.putpixel((0, y), (
        int(top[0] + (bot[0] - top[0]) * t),
        int(top[1] + (bot[1] - top[1]) * t),
        int(top[2] + (bot[2] - top[2]) * t),
        255,
    ))
grad = line.resize((S, S))

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
mask = Image.new("L", (S, S), 0)
md = ImageDraw.Draw(mask)
m = 48
rad = 232
md.rounded_rectangle([m, m, S - m, S - m], radius=rad, fill=255)
img.paste(grad, (0, 0), mask)

d = ImageDraw.Draw(img)

# subtle top highlight for depth
hl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
hd = ImageDraw.Draw(hl)
hd.rounded_rectangle([m, m, S - m, S - m], radius=rad, outline=(255, 255, 255, 70), width=10)
img.alpha_composite(hl)

font_path = r"C:\Windows\Fonts\msyhbd.ttc"
if not os.path.exists(font_path):
    font_path = r"C:\Windows\Fonts\msyh.ttc"
fs = 430
font = ImageFont.truetype(font_path, fs, index=0)
text = "A\u6587"  # A 文
bbox = d.textbbox((0, 0), text, font=font)
w = bbox[2] - bbox[0]
h = bbox[3] - bbox[1]
x = (S - w) / 2 - bbox[0]
y = (S - h) / 2 - bbox[1] - 12
d.text((x, y), text, font=font, fill=(255, 255, 255, 255))

base = img.resize((256, 256), Image.LANCZOS)
base.save(os.path.join(ASSETS, "icon_preview.png"))
base.save(
    os.path.join(ASSETS, "icon.ico"),
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
print("WROTE", os.path.join(ASSETS, "icon.ico"))
print("WROTE", os.path.join(ASSETS, "icon_preview.png"))
print("ICO bytes:", os.path.getsize(os.path.join(ASSETS, "icon.ico")))