#!/usr/bin/env python3
"""按设计稿 10C.3 几何生成 dd-run 应用图标（多尺寸 .ico）。

规格来源：cmdpal-ui-mockups.html §10C.3「应用图标设计」（v4.5 定稿）：
- 设计网格 32×32（×8 = 256 母版光栅化）；
- 圆角 7.5（≈23%，Win11 惯例）→ 256 档 60px；整体 1px（8px）内缩防边缘裁切；
- 底色线性渐变 135°：#3AA0FF → #0F5FC0；
- 主标「DD 快进」：每个大写 D = 竖杠宽 3 + 右半圆 r6（mark 高 12，y 10–22），
  D 间距 2，整组居中 x 6–26，白色实心；
- 小尺寸：256 母版 LANCZOS 缩到 48/32/24/20/16（16 = 托盘逻辑尺寸）。

产出：
- crates/dd-gui/assets/app.ico（BMP 条目，LoadImageW 全版本兼容；
  16/20/24/32 = D22 DPI 档，48 富余）；
- tools/gen_icon_preview.png（256px 预览，人工核验用）。

用法：python tools/gen_icon.py（需 Pillow）。
"""

from pathlib import Path

from PIL import Image, ImageDraw

GRID = 32          # 设计网格
SCALE = 8          # 256 母版光栅化倍率
MASTER = GRID * SCALE

GRAD_FROM = (0x3A, 0xA0, 0xFF)
GRAD_TO = (0x0F, 0x5F, 0xC0)
RADIUS = 7.5 * SCALE       # ≈23% 圆角
INSET = 1 * SCALE          # 1px 内缩
BAR_W = 3 * SCALE          # D 竖杠宽
BOWL_R = 6 * SCALE         # 半圆半径
MARK_TOP = 10 * SCALE      # mark 顶 y
MARK_BOT = 22 * SCALE      # mark 底 y
D1_BAR_X = 6 * SCALE       # D1 竖杠左缘
D_GAP = 2 * SCALE          # D 间距（D1 半圆右缘 → D2 竖杠左缘）

SIZES = [48, 32, 24, 20, 16]
OUT_ICO = Path(__file__).resolve().parent.parent / "crates/dd-gui/assets/app.ico"
OUT_PNG = Path(__file__).resolve().parent / "gen_icon_preview.png"


def gradient_135(size: int) -> Image.Image:
    """对角线（左上→右下）线性渐变：t = (x + y) / (2·(size-1))。"""
    img = Image.new("RGB", (size, size))
    px = img.load()
    denom = 2 * (size - 1)
    for y in range(size):
        for x in range(size):
            t = (x + y) / denom
            px[x, y] = tuple(
                round(a + (b - a) * t) for a, b in zip(GRAD_FROM, GRAD_TO)
            )
    return img


def build_master() -> Image.Image:
    """256 母版：渐变底（圆角遮罩）+ 白色 DD 字标。"""
    icon = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))

    mask = Image.new("L", (MASTER, MASTER), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (INSET, INSET, MASTER - INSET, MASTER - INSET),
        radius=RADIUS,
        fill=255,
    )
    icon.paste(gradient_135(MASTER), (0, 0), mask)

    mark = Image.new("L", (MASTER, MASTER), 0)
    d = ImageDraw.Draw(mark)
    cy = (MARK_TOP + MARK_BOT) // 2
    bowl_bbox_cache = None
    # D1：竖杠 [x6, x9] + 右半圆（圆心 x9，r6）
    d.rectangle((D1_BAR_X, MARK_TOP, D1_BAR_X + BAR_W, MARK_BOT), fill=255)
    bbox = (
        D1_BAR_X + BAR_W - BOWL_R, cy - BOWL_R,
        D1_BAR_X + BAR_W + BOWL_R, cy + BOWL_R,
    )
    d.pieslice(bbox, 270, 90, fill=255)  # 右半圆（12 点 → 3 点 → 6 点）
    # D2：整体右移（杠宽 + 半圆直径 + 间距）
    shift = BAR_W + BOWL_R + D_GAP
    d.rectangle(
        (D1_BAR_X + shift, MARK_TOP, D1_BAR_X + shift + BAR_W, MARK_BOT),
        fill=255,
    )
    bowl_bbox_cache = (
        D1_BAR_X + shift + BAR_W - BOWL_R, cy - BOWL_R,
        D1_BAR_X + shift + BAR_W + BOWL_R, cy + BOWL_R,
    )
    d.pieslice(bowl_bbox_cache, 270, 90, fill=255)
    icon.paste((255, 255, 255, 255), (0, 0), mark)
    return icon


def main() -> None:
    master = build_master()
    sizes = [(s, s) for s in SIZES]
    frames = {s: master.resize((s, s), Image.LANCZOS) for s in SIZES}
    OUT_ICO.parent.mkdir(parents=True, exist_ok=True)
    frames[SIZES[0]].save(
        OUT_ICO, format="ICO", bitmap_format="bmp", sizes=sizes,
        append_images=[frames[s] for s in SIZES[1:]],
    )
    master.save(OUT_PNG)
    print(f"✅ {OUT_ICO}（{OUT_ICO.stat().st_size} bytes）")
    print(f"✅ {OUT_PNG}")


if __name__ == "__main__":
    main()
