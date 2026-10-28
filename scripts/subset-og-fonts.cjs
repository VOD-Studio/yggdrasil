#!/usr/bin/env bun
// 裁剪分享图字体；源字体来自 notofonts/noto-cjk 的 Serif/SubsetOTF/SC 与 Sans/SubsetOTF/SC。
// 先在 libs/ 执行 bun install，再运行：bun scripts/subset-og-fonts.cjs <源字体目录>
// 输出到 src/og/fonts/：常用汉字、英文与标点；Bold 仅保留 ASCII。
const path = require('node:path');

const OUT = path.join(__dirname, '../src/og/fonts');
const HB_MEMORY_MODE_WRITABLE = 2;
const HB_SUBSET_SETS_NAME_ID = 4;
const HB_SUBSET_SETS_LAYOUT_FEATURE_TAG = 6;
const HB_SUBSET_FLAGS_NOTDEF_OUTLINE = 0x40;

function ranges(...spans) {
  let text = '';
  for (const [lo, hi] of spans) {
    for (let code = lo; code <= hi; code++) text += String.fromCodePoint(code);
  }
  return text;
}

function gb2312Hanzi() {
  const decoder = new TextDecoder('gb2312', { fatal: true });
  const bytes = new Uint8Array(2);
  let text = '';
  for (let hi = 0xb0; hi <= 0xf7; hi++) {
    bytes[0] = hi;
    for (let lo = 0xa1; lo <= 0xfe; lo++) {
      // WHATWG 的 gb2312 实为 GBK；D7FA–D7FE 在 GB2312 中未分配，不能解成私用字符。
      if (hi === 0xd7 && lo >= 0xfa) continue;
      bytes[1] = lo;
      text += decoder.decode(bytes);
    }
  }
  return text;
}

// 直接调用 HarfBuzz 的裁剪 WASM，保留 .notdef 轮廓、全部名称 ID 与 CFF 子程序。
function subsetFont(hb, data, text) {
  const input = hb.hb_subset_input_create_or_fail();
  if (!input) throw new Error('无法创建字体裁剪参数');
  let pointer = 0;
  let blob = 0;
  let face = 0;
  let subset = 0;
  let result = 0;
  try {
    pointer = hb.malloc(data.byteLength);
    if (!pointer) throw new Error('无法分配字体裁剪内存');
    new Uint8Array(hb.memory.buffer).set(data, pointer);
    blob = hb.hb_blob_create(pointer, data.byteLength, HB_MEMORY_MODE_WRITABLE, 0, 0);
    face = hb.hb_face_create(blob, 0);
    if (face === hb.hb_face_get_empty()) throw new Error('源文件不是有效字体');

    hb.hb_subset_input_set_flags(input,
      hb.hb_subset_input_get_flags(input) | HB_SUBSET_FLAGS_NOTDEF_OUTLINE);
    const names = hb.hb_subset_input_set(input, HB_SUBSET_SETS_NAME_ID);
    hb.hb_set_clear(names);
    hb.hb_set_invert(names);
    const features = hb.hb_subset_input_set(input, HB_SUBSET_SETS_LAYOUT_FEATURE_TAG);
    hb.hb_set_clear(features);
    for (const tag of ['kern', 'palt', 'vert', 'liga']) {
      const value = [...tag].reduce((value, char) => (value << 8) | char.charCodeAt(0), 0);
      hb.hb_set_add(features, value);
    }
    const unicodes = hb.hb_subset_input_unicode_set(input);
    for (const char of text) hb.hb_set_add(unicodes, char.codePointAt(0));

    subset = hb.hb_subset_or_fail(face, input);
    if (!subset) throw new Error('字体裁剪失败');
    result = hb.hb_face_reference_blob(subset);
    const size = hb.hb_blob_get_length(result);
    if (!size) throw new Error('字体裁剪结果为空');
    const offset = hb.hb_blob_get_data(result, 0);
    // WASM 内存可能增长，必须重新取视图，并在释放结果前复制到 Bun 管理的内存。
    return Buffer.from(new Uint8Array(hb.memory.buffer, offset, size));
  } finally {
    if (result) hb.hb_blob_destroy(result);
    if (subset) hb.hb_face_destroy(subset);
    if (face) hb.hb_face_destroy(face);
    if (blob) hb.hb_blob_destroy(blob);
    if (pointer) hb.free(pointer);
    hb.hb_subset_input_destroy(input);
  }
}

async function main() {
  if (process.argv.length !== 3) {
    throw new Error('用法：bun scripts/subset-og-fonts.cjs <源字体目录>');
  }
  const source = path.resolve(process.argv[2]);
  const wasm = require.resolve('../libs/node_modules/harfbuzzjs/dist/harfbuzz-subset.wasm');
  const { instance: { exports: hb } } = await WebAssembly.instantiate(await Bun.file(wasm).arrayBuffer());
  hb._initialize();

  const ascii = ranges([0x20, 0x7e]);
  const common = ascii + ranges(
    [0xa0, 0xff], // 拉丁补充。
    [0x2010, 0x2027], // 连字符、引号、省略号、间隔号。
    [0x2030, 0x203a],
    [0x2190, 0x2193], // 箭头。
    [0x2212, 0x2212],
    [0x3000, 0x301f], // 中文标点。
    [0xff01, 0xff5e], // 全角。
  ) + '·×÷°№℃';
  const full = common + gb2312Hanzi();
  for (const [name, text] of [
    ['NotoSerifSC-Medium.otf', full],
    ['NotoSansSC-Regular.otf', full],
    ['NotoSansSC-Bold.otf', ascii],
  ]) {
    const data = new Uint8Array(await Bun.file(path.join(source, name)).arrayBuffer());
    const output = subsetFont(hb, data, text);
    await Bun.write(path.join(OUT, name), output);
    console.log(`${name}: ${Math.round(output.byteLength / 1024)} KiB`);
  }
}

main().catch(error => {
  console.error(error.message);
  process.exitCode = 1;
});
