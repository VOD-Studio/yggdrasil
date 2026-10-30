//! merman 输出 SVG 的后处理：子图重叠检测与子图标题置顶。

const CLUSTERS: &str = r#"<g class="clusters">"#;

/// 返回从 `start`（指向某个 `<g` 开标签）起、与之配对的 `</g>` 结束位置（不含）。
fn balanced_g_end(svg: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < svg.len() {
        i += svg[i..].find('<')?;
        let tag = &svg[i..];
        if tag.starts_with("</g>") {
            depth = depth.checked_sub(1)?;
            i += 4;
            if depth == 0 {
                return Some(i);
            }
        } else if tag.starts_with("<g ") || tag.starts_with("<g>") {
            let close = tag.find('>')?;
            // 自闭合 `<g .../>` 不改变深度。
            if !tag[..close].ends_with('/') {
                depth += 1;
            }
            i += close + 1;
        } else {
            i += 1;
        }
    }
    None
}

/// 取 `<g class="cluster" ...><rect ... x= y= width= height=>` 的矩形，按 clusters 组分组。
fn cluster_rects(svg: &str) -> Vec<Vec<[f64; 4]>> {
    fn attr(tag: &str, name: &str) -> Option<f64> {
        let key = format!(r#" {name}=""#);
        let at = tag.find(&key)? + key.len();
        tag[at..].split('"').next()?.parse().ok()
    }
    svg.split(CLUSTERS)
        .skip(1)
        .map(|group| {
            group
                .split(r#"<g class="cluster" id=""#)
                .skip(1)
                .filter_map(|c| {
                    let rect = &c[c.find("<rect")?..];
                    let tag = &rect[..rect.find('>')?];
                    Some([
                        attr(tag, "x")?,
                        attr(tag, "y")?,
                        attr(tag, "width")?,
                        attr(tag, "height")?,
                    ])
                })
                .collect()
        })
        .collect()
}

/// 同一 clusters 组内两个子图框相交超过 0.5px 且互不包含（不是嵌套关系）即视为重叠。
///
/// Dagre 按子节点算子图大小，不考虑标题；Mermaid 事后把框撑到标题宽度，左右各外扩一截，
/// 相邻子图就可能重叠。调用方据此加大 `nodeSpacing` 重新排版。
pub(super) fn has_cluster_overlap(svg: &str) -> bool {
    let contains = |p: &[f64; 4], q: &[f64; 4]| {
        p[0] <= q[0] && p[1] <= q[1] && p[0] + p[2] >= q[0] + q[2] && p[1] + p[3] >= q[1] + q[3]
    };
    cluster_rects(svg).iter().any(|rects| {
        rects.iter().enumerate().any(|(i, a)| {
            rects[i + 1..].iter().any(|b| {
                let ix = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
                let iy = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
                ix > 0.5 && iy > 0.5 && !contains(a, b) && !contains(b, a)
            })
        })
    })
}

/// 在每个 `<g class="clusters">` 内，把所有子图标题移到全部子图底框之后绘制，标题总在最上层。
///
/// 后画的子图底色会盖住先画的子图标题（Mermaid 原版同样如此）。子图 `<g>` 无 transform，
/// 标题坐标相对所在 clusters 组不变，移动不改变位置；不跨组移动，因为不同 root 组可能各带
/// transform。外包一层同 class 的 `<g>` 以保留 `.cluster span` 等样式。
pub(super) fn lift_cluster_labels(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 128);
    let mut rest = svg;
    while let Some(start) = rest.find(CLUSTERS) {
        let Some(end) = balanced_g_end(rest, start) else {
            break;
        };
        out.push_str(&rest[..start]);
        lift_labels_in_group(&rest[start..end], &mut out);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// 处理一个完整的 clusters 组；解析失败时原样输出该组。
fn lift_labels_in_group(group: &str, out: &mut String) {
    const LABEL: &str = r#"<g class="cluster-label""#;
    let inner_end = group.len() - "</g>".len();
    let mut labels = String::new();
    let start_len = out.len();
    out.push_str(CLUSTERS);
    let mut i = CLUSTERS.len();
    while let Some(rel) = group[i..inner_end].find(LABEL) {
        let at = i + rel;
        let Some(label_end) = balanced_g_end(group, at) else {
            out.truncate(start_len);
            out.push_str(group);
            return;
        };
        out.push_str(&group[i..at]);
        labels.push_str(r#"<g class="cluster" data-look="classic">"#);
        labels.push_str(&group[at..label_end]);
        labels.push_str("</g>");
        i = label_end;
    }
    out.push_str(&group[i..inner_end]);
    out.push_str(&labels);
    out.push_str("</g>");
}

/// 最终修正：去掉末尾换行（`strip_dark_variants` 依赖 `</svg></div></figure>` 紧邻）；
/// 子图标题置顶；HTML 标签的 foreignObject 允许溢出可见
/// （字体实际宽度略超测量值时不裁掉文字，与 Mermaid 前端渲染时的修正一致）。
pub(super) fn finish(svg: &str) -> String {
    lift_cluster_labels(svg.trim_end())
        .replace("<foreignObject ", r#"<foreignObject overflow="visible" "#)
}
