"""裁剪分享图字体：Noto Serif SC Medium（标题、标语）与 Noto Sans SC Regular / Bold（其余文字）。

分享图在服务端渲染，运行镜像没有中文字体，字体随二进制嵌入，因此只保留常用字符：
ASCII、拉丁补充、常用标点与符号、全角字符，以及 GB 2312 收录的 6763 个汉字。
Bold 只用于文字标与英文，裁到 ASCII。缺字时服务端回退为站点默认图。

用法（源字体来自 github.com/notofonts/noto-cjk 的 SubsetOTF/SC）：

    uv run --with fonttools python scripts/subset-og-fonts.py <源字体目录>
"""

import sys
from pathlib import Path

from fontTools import subset

OUT = Path(__file__).resolve().parent.parent / "src" / "og" / "fonts"


def gb2312_hanzi():
    chars = []
    for hi in range(0xB0, 0xF8):
        for lo in range(0xA1, 0xFF):
            try:
                chars.append(bytes([hi, lo]).decode("gb2312"))
            except UnicodeDecodeError:
                pass
    return chars


def ranges(*spans):
    return [chr(c) for lo, hi in spans for c in range(lo, hi + 1)]


ASCII = ranges((0x20, 0x7E))
COMMON = ASCII + ranges(
    (0xA0, 0xFF),  # 拉丁补充
    (0x2010, 0x2027),  # 连字符、引号、省略号、间隔号
    (0x2030, 0x203A),
    (0x2190, 0x2193),  # 箭头
    (0x2212, 0x2212),
    (0x3000, 0x301F),  # 中文标点
    (0xFF01, 0xFF5E),  # 全角
) + ["·", "×", "÷", "°", "№", "℃"]


def build(src, dst, chars):
    options = subset.Options()
    options.layout_features = ["kern", "palt", "vert", "liga"]
    options.name_IDs = ["*"]
    options.notdef_outline = True
    options.desubroutinize = False
    font = subset.load_font(str(src), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(text="".join(chars))
    subsetter.subset(font)
    subset.save_font(font, str(OUT / dst), options)
    print(f"{dst}: {(OUT / dst).stat().st_size / 1024:.0f} KiB")


def main():
    src = Path(sys.argv[1])
    full = COMMON + gb2312_hanzi()
    build(src / "NotoSerifSC-Medium.otf", "NotoSerifSC-Medium.otf", full)
    build(src / "NotoSansSC-Regular.otf", "NotoSansSC-Regular.otf", full)
    build(src / "NotoSansSC-Bold.otf", "NotoSansSC-Bold.otf", ASCII)


if __name__ == "__main__":
    main()
