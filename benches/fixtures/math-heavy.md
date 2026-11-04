> 一个用 Bevy 0.19 写就的实时 Kerr（旋转）黑洞渲染器。每一个像素都在 GPU 上独立积分一条弯曲时空中的测地线，引力透镜、爱因斯坦环、多普勒增亮的吸积盘、自旋驱动的相对论喷流——全部从物理公式自然涌现，而非贴图伪造。它在桌面与 WebGPU 上以 60 FPS 运行。

写下这个渲染器的过程，本质上是在和广义相对论的方程较劲——让它们在 GPU 上实时可解，又在物理保真与渲染预算之间走出一条平衡线。本文是对这次工程内核的解剖：不是使用说明，而是把每一条物理公式、每一处工程妥协都摊开来讲清楚。涉及到的数学，都尽量给出推导脉络，而不是只甩一个结论。

---

## 一、问题的本质：为什么黑洞渲染很难

渲染一个黑洞，难点不在于"画一个黑色的圆"——而在于**光本身不再走直线**。

在黑洞附近，时空被质量弯曲。一束从远方恒星射来的光，其轨迹会被引力折射，绕黑洞缠绕，甚至绕好几圈才到达你的眼睛。这意味着：屏幕上每一个像素的颜色，**不能由一条直线射线与场景求交得到**，而要由一条**弯曲的测地线**决定。这条测地线的形状取决于黑洞的质量、自旋，以及光线的撞击参数（impact parameter）——即光线若不受偏折时与黑洞中心的最近距离。

于是渲染退化为一个**边值问题**：对每个像素，从相机出发反向追踪一条测地线，在弯曲时空中数值积分，直到它（a）坠入事件视界——该像素为黑；（b）逃逸到无穷远——该像素采样恒星背景；或（c）途中穿过吸积盘——该像素叠加盘的辐射。

这个积分没有解析解（Kerr 时空的测地线方程不可积为初等函数），必须**数值求解**。而你要在 200 万像素上、每秒 60 次地求解它。这就是 singularity-rs 要攻克的根本矛盾：**广义相对论的精度 vs. 实时光栅化的预算**。

---

## 二、物理核心：从 Schwarzschild 到 Kerr 的精确退化

### 2.1 自然单位制：`Rs = 1`

整个项目贯穿一个约定：**以史瓦西半径为单位**。

史瓦西半径 $R_s$ 是黑洞的"特征尺度"——对一个不旋转、不带电的黑洞，它就是事件视界的半径，定义为：

$$R_s = \frac{2GM}{c^2}$$

其中 $G$ 是引力常数，$M$ 是黑洞质量，$c$ 是光速。在数值实现里取 $R_s = 1$，于是由上式反推质量

$$M = \frac{R_s}{2} = 0.5$$

这就是代码里到处出现的 `let m = 0.5;` 的来历。所有距离、速度、频率都以 $R_s$ 为标尺。这不仅消除了量纲混乱，更让物理常数在 Rust 与 WGSL 两侧以同样的字面量出现——这是 CPU↔GPU 镜像一致性的基础。

在这个单位制下，史瓦西黑洞的**临界撞击参数** $b_{\text{crit}}$ 是一个关键量：它是刚好能"擦边"逃逸的光线的撞击参数，小于它则被捕获。精确值为：

$$b_{\text{crit}} = \frac{3\sqrt{3}}{2}\, R_s \approx 2.598$$

这个值的来历值得展开。在史瓦西时空中，零测地线（光线）满足守恒的能量 $E$ 与角动量 $L$，撞击参数 $b = L/E$。结合测地线方程，光线的有效势能允许在 $r = 1.5\,R_s$ 处存在一个**不稳定圆轨道**（光子球）。一条刚好在光子球外缘掠过的光线对应最大的可逃逸撞击参数，解轨道方程即得上式。

在渲染器里，$b_{\text{crit}}$ 不是硬编码的——它从积分中自然涌现：撞击参数 $b < b_{\text{crit}}$ 的光线被黑洞捕获，$b > b_{\text{crit}}$ 的光线逃逸。但它被写成了 `physics.rs` 里的常量（因为 `f32::sqrt` 不是 `const fn`，无法在编译期算），并由测试守护：

```rust
pub const BCRIT: f32 = 2.598_076;
```

> 试运行:在 $R_s=1$ 的自然单位制下,验证临界撞击参数的解析值。

```rust runnable
// 自然单位制:Rs = 1 (史瓦西半径),M = Rs/2 = 0.5
const RS: f32 = 1.0;

// Schwarzschild 临界撞击参数:b < b_crit → 捕获;b > b_crit → 逃逸。
const BCRIT: f32 = 2.598_076;  // = 3*sqrt(3)/2 * Rs

fn main() {
    let analytic = 1.5 * 3f32.sqrt() * RS;
    println!("b_crit 解析值   = {:.5}", analytic);
    println!("b_crit 代码常量 = {:.5}", BCRIT);
    println!("误差 = {:.2e}", (analytic - BCRIT).abs());
    assert!((analytic - BCRIT).abs() < 0.01);
    println!("\n校验通过:b=2.0<2.598→捕获,b=3.0>2.598→逃逸");
}
```

### 2.2 弯曲加速度：一条公式的两种命运

整个渲染器的物理灵魂，浓缩在 `deriv` 函数（着色器）/ `kerr_bending_accel`（CPU 镜像）里：

```rust
let radial = -1.5 * RS * h2 / r5 * pos;          // 径向弯曲（Schwarzschild）
let drag   =  2.0 * M * a / r3 * spin_axis.cross(dir);  // 参考系拖曳（Lense-Thirring）
let accel  = radial + drag;
```

先拆开看每一项。

**角动量** $h$**。** 设光线位置 $\vec{p}$，方向 $\vec{d}$（单位向量），定义

$$\vec{h} = \vec{p} \times \vec{d}, \qquad h^2 = \vec{h}\cdot\vec{h}$$

在球对称的史瓦西时空中，$h$ 就是每单位能量的轨道角动量，是一个守恒量。代码里 `let h = pos.cross(dir); let h2 = h.dot(h);` 即此。

**径向弯曲项。** 这一项把光线"拉向"黑洞中心，方向沿 $-\vec{p}$（指向原点）：

$$\vec{a}_{\text{radial}} = -\frac{3}{2}\,R_s\,\frac{h^2}{r^5}\,\vec{p}$$

其中 $r = |\vec{p}|$。它的物理来历：光线的偏折可以由测地线方程的二阶近似得到，二阶导 $\ddot{\vec{p}}$ 中主导项正比于 $h^2/r^5$ 乘以位置向量。$r^5 = r \cdot r^4$ 在分母，保证了 $r\to 0$ 时偏折急剧增强（强场），$r\to\infty$ 时迅速衰减（弱场近似牛顿）。代码里 `r5 = max(r*r*r*r*r, 1e-6)` 的 `max` 是防止除零的数值护栏。

**参考系拖曳项（Lense-Thirring 前导项）。** 当黑洞自旋 $\chi = a/M \neq 0$，旋转的质量会"拖动"周围的时空一起转。这一项沿自旋轴 $\hat{a}$ 的横向方向施加偏转：

$$\vec{a}_{\text{drag}} = \frac{2Ma}{r^3}\,(\hat{a} \times \vec{d})$$

其中 $a = \chi M$ 是 Kerr 自旋长度（几何化），$\hat{a}$ 是自旋轴方向（代码里固定为 $+Y$）。叉积 $\hat{a}\times\vec{d}$ 给出一个垂直于自旋轴和光线方向的横向分量，这正是"拖曳"的方向。$r^3$ 在分母，意味着拖曳随距离立方衰减。

**精确退化。** 这个公式有一个极其优雅的性质：**当** $\chi = 0$ **时，**$a = 0$**，drag 项精确归零，积分器退化为纯粹的 Schwarzschild**。这不是近似，不是 `if` 分支，而是数学上的精确退化。自旋参数 $\chi$ 驱动着两个可观测效应：

1. **参考系拖曳（Lense-Thirring）**：drag 项使光线的方向产生横向偏转，破坏了球对称，让光晕相对视线轴发生剪切。
2. **自旋依赖的捕获半径**：见下一节。

两项的 $\chi = 0$ 极限都精确回到 Schwarzschild——这被 14 个单元测试逐条断言。

### 2.3 自旋依赖的捕获半径：Kerr 视界

旋转黑洞的事件视界比史瓦西的小。Kerr 度规的视界半径 $r_+$ 为：

$$r_+ = M + \sqrt{M^2 - a^2}$$

代入 $M = 0.5$、$a = \chi M = 0.5\chi$：

- $\chi = 0$：$r_+ = M + M = 2M = R_s = 1$（史瓦西）；
- $\chi = 1$（极端自旋）：$r_+ = M + 0 = M = 0.5$。

```rust
pub fn kerr_horizon(chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    m + (m * m - a * a).max(0.0).sqrt()
}
```

> 试运行:Kerr 视界 $r_+$ 如何随自旋从 $R_s$ 收缩到 $M$。

```rust runnable
const M: f32 = 0.5;

/// Kerr 事件视界半径 r+(chi),单位 Rs。chi=0 → Rs=1,chi=1 → M=0.5。
fn kerr_horizon(chi: f32) -> f32 {
    let a = chi * M;
    M + (M * M - a * a).max(0.0).sqrt()
}

fn main() {
    println!(" chi     r+(chi)   说明");
    for chi in [0.0_f32, 0.25, 0.5, 0.75, 0.9, 0.99, 1.0] {
        let r = kerr_horizon(chi);
        let note = if chi == 0.0 { " <- Schwarzschild" }
                   else if (chi - 1.0).abs() < 1e-6 { " <- 极端 Kerr" } else { "" };
        println!(" {:.2}    {:.5}{}", chi, r, note);
    }
    assert!((kerr_horizon(0.0) - 1.0).abs() < 1e-5, "chi=0 应为 1 Rs");
    assert!((kerr_horizon(1.0) - 0.5).abs() < 1e-5, "chi=1 应为 0.5 Rs");
    println!("\n极限校验通过:chi=0→1 Rs,chi=1→0.5 Rs (视界随自旋收缩)");
}
```

`.max(0.0)` 是数值护栏：极端自旋下 $M^2 - a^2$ 理论上为零，浮点误差可能让它略负。光线穿过 $r < r_+(\chi)$ 即判定捕获。

### 2.4 ISCO：自旋如何塑造吸积盘的内缘

吸积盘的内边缘不是任意设定的——它追踪 Kerr 的**最内稳定圆轨道（Innermost Stable Circular Orbit, ISCO）**。在 ISCO 以内，物质无法维持稳定圆轨道，必然螺旋坠入黑洞，所以那里不再有盘的物质。

ISCO 没有简单的初等形式，但有 Bardeen-Press-Teukolsky (1972) 给出的闭式解。定义两个中间量：

$$Z_1 = 1 + \sqrt[3]{1-\chi^2}\,\Big(\sqrt[3]{1+\chi} + \sqrt[3]{1-\chi}\Big)$$

$$Z_2 = \sqrt{3\chi^2 + Z_1^2}$$

则顺行（prograde，与黑洞同向旋转）轨道的 ISCO 为：

$$r_{\text{ISCO}} = M\left(3 + Z_2 - \sqrt{(3-Z_1)(3+Z_1+2Z_2)}\right)$$

几个极限值便于校验：

- $\chi = 0$：$Z_1 = 3$，$Z_2 = 3$，代入得 $r_{\text{ISCO}} = M(3+3-\sqrt{0})$ … 化简后为 $6M = 3\,R_s$；
- $\chi = 1$：$r_{\text{ISCO}} = M = 0.5\,R_s$。

```rust
pub fn kerr_isco(chi: f32) -> f32 {
    let m = 0.5;
    let z1 = 1.0 + (1.0 - chi * chi).cbrt() * ((1.0 + chi).cbrt() + (1.0 - chi).cbrt());
    let z2 = (3.0 * chi * chi + z1 * z1).sqrt();
    m * (3.0 + z2 - ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt())
}
```

> 试运行:ISCO 如何随自旋 $\chi$ 从 $3\,R_s$ 收缩到 $0.5\,R_s$。

```rust runnable
const M: f32 = 0.5;

/// Kerr 顺行最内稳定圆轨道 ISCO (Bardeen-Press-Teukolsky 1972)。
/// chi=0 → 6M = 3 Rs;chi=1 → M = 0.5 Rs。
fn kerr_isco(chi: f32) -> f32 {
    let z1 = 1.0 + (1.0 - chi*chi).cbrt() * ((1.0+chi).cbrt() + (1.0-chi).cbrt());
    let z2 = (3.0*chi*chi + z1*z1).sqrt();
    M * (3.0 + z2 - ((3.0 - z1)*(3.0 + z1 + 2.0*z2)).sqrt())
}

fn main() {
    println!(" chi     ISCO(Rs)");
    for chi in [0.0_f32, 0.25, 0.5, 0.75, 0.9, 0.99, 1.0] {
        println!(" {:.2}    {:.5}", chi, kerr_isco(chi));
    }
    assert!((kerr_isco(0.0) - 3.0).abs() < 1e-4, "chi=0 应为 3 Rs");
    assert!((kerr_isco(1.0) - 0.5).abs() < 1e-3, "chi=1 应为 0.5 Rs");
    println!("\n校验通过:chi=0→3 Rs,chi=1→0.5 Rs");
}
```

这里有一个精妙的工程设计决策：`params.rs` 里**有**一个 `disk_inner` 字段，但 `mirror_params`（每帧把参数镜像到 GPU uniform 的函数）**故意覆盖**它：

```rust
// disk_inner 是自旋导出的（Kerr ISCO）；params.disk_inner 字段被忽略。
u.disk_inner = crate::physics::kerr_isco(params.spin);
```

`disk_inner` 字段存在的唯一理由是 UI 默认读数的便利，运行时**永远**被 ISCO 公式覆盖。这消除了"用户手动设了个内缘但物理上不自洽"的整类 bug——单一数据源，物理保证。

---

## 三、积分器：GPU 上的自适应 RK45

### 3.1 为什么不用固定步长

黑洞附近的时空曲率变化剧烈：在事件视界附近，曲率极高，光线急剧偏转，需要极小的步长才能准确积分；而在远离黑洞的平直区域，大步长即可。固定步长面临两难——步长太小则在平直区浪费算力，步长太大则在强场区积分失准，捕获/逃逸判定出错。

答案是**自适应步长**：根据每步的局部截断误差估计，动态放大或缩小步长。

singularity-rs 选择了 **Dormand-Prince RK45** 方法——这恰好是 MATLAB `ode45` 背后的方法。它属于嵌入式 Runge-Kutta 家族，核心思想是用同一组中间求值同时算出两个不同阶的解，它们的差就是误差的"免费"估计。

### 3.2 RK45 的数学原理

考虑常微分方程初值问题 $\dot{y} = f(t, y)$。一个 $s$ 级显式 Runge-Kutta 方法形如：

$$y_{n+1} = y_n + h\sum_{i=1}^{s} b_i k_i$$

其中各级 $k_i$ 为：

$$k_i = f\!\left(t_n + c_i h,\; y_n + h\sum_{j=1}^{i-1} a_{ij} k_j\right)$$

系数 $c_i$、$a_{ij}$、$b_i$ 构成 **Butcher 表**。Dormand-Prince 的精妙之处在于：它设计了两套权重 $b_i$（5 阶）和 $\hat{b}_i$（4 阶），共用**完全相同的 6 个** $k_i$ **求值**，于是：

$$y_{n+1}^{(5)} = y_n + h\sum_{i} b_i k_i \quad(\text{5 阶，用于推进})$$

$$y_{n+1}^{(4)} = y_n + h\sum_{i} \hat{b}_i k_i \quad(\text{4 阶，仅用于误差估计})$$

误差估计为两者之差：

$$\text{err} = \left|y_{n+1}^{(5)} - y_{n+1}^{(4)}\right| = h\left|\sum_{i}(b_i - \hat{b}_i)k_i\right|$$

这相当于"用 6 次函数求值的代价，同时得到了 5 阶解和 4 阶误差监控"——没有一次额外的求值浪费。

代码里就是这张 Butcher 表的逐字翻译。以第 3 级为例：

```rust
let p3 = pos + (k1p * 0.075 + k2p * 0.225) * dt;   // c3 = a31 + a32 = 0.3
```

其中 $a_{31} = 0.075$、$a_{32} = 0.225$ 是 DOPRI 表的精确系数。6 级、5 阶权重 $(37/378,\,0,\,250/621,\,125/594,\,512/1771,\,0)$、4 阶权重 $(2825/27648,\,0,\,18575/48384,\,13525/55296,\,277/14336,\,1/4)$——这些分数在 Rust 与 WGSL 两侧逐字一致。

### 3.3 步长控制律

有了误差估计 $\text{err}$，如何调整步长？标准做法是：希望误差接近（但不超过）容差 $\text{tol}$。对于 4 阶误差估计，局部误差按 $h^5$ 缩放（$\text{err} \propto h^{p+1}$，$p=4$），所以最优步长满足：

$$\frac{\text{err}_{\text{new}}}{\text{tol}} \approx \left(\frac{h_{\text{new}}}{h}\right)^5 \cdot \frac{\text{err}}{\text{tol}}$$

令其等于 1，解出：

$$h_{\text{new}} = h \cdot \left(\frac{\text{tol}}{\text{err}}\right)^{1/5}$$

指数 $1/5 = 0.2$ 正是代码里 `powf(0.2)` 的来历。实际实现加了上下限钳制和 $0.2$ 的指数：

```rust
dt = (dt * (tol / err.max(1e-12)).powf(0.2)).clamp(dt_min, dt_max);
```

这保证步长在平直区膨胀（上限 `dt_max = 4×dt_init`）、在强场区收缩（下限 `dt_min = 0.25×dt_init`）。

> 试运行:用嵌入式 RK45 积分 $y'=y$($y(0)=1$,解析解 $y(1)=e$),观察自适应步长如何拒绝、接受、细化。

```rust runnable
// 嵌入式 Runge-Kutta 4(5):一个 6 级表同时给出 5 阶解和 4 阶误差估计。
fn deriv(_t: f64, y: f64) -> f64 { y }

// 一步 Cash-Karp RK45:返回 (5 阶解, |5阶−4阶| 误差估计)
fn rk45_step(t: f64, y: f64, h: f64) -> (f64, f64) {
    let k1 = deriv(t,             y);
    let k2 = deriv(t + 1.0/5.0*h, y + h*( 1.0/5.0*k1));
    let k3 = deriv(t + 3.0/10.0*h,y + h*( 3.0/40.0*k1 + 9.0/40.0*k2));
    let k4 = deriv(t + 3.0/5.0*h, y + h*( 3.0/10.0*k1 - 9.0/10.0*k2 + 6.0/5.0*k3));
    let k5 = deriv(t + h,         y + h*(-11.0/54.0*k1 + 5.0/2.0*k2 - 70.0/27.0*k3 + 35.0/27.0*k4));
    let k6 = deriv(t + 7.0/8.0*h, y + h*(1631.0/55296.0*k1 + 175.0/512.0*k2 + 575.0/13824.0*k3 + 44275.0/110592.0*k4 + 253.0/4096.0*k5));
    let y5 = y + h*(37.0/378.0*k1 + 250.0/621.0*k3 + 125.0/594.0*k4 + 512.0/1771.0*k6);
    let y4 = y + h*(2825.0/27648.0*k1 + 18575.0/48384.0*k3 + 13525.0/55296.0*k4 + 277.0/14336.0*k5 + 1.0/4.0*k6);
    (y5, (y5 - y4).abs())
}

fn main() {
    let mut t = 0.0_f64;
    let mut y = 1.0_f64;
    let mut h = 0.2_f64;
    let tol = 1e-10;
    let mut accepted = 0;
    let mut rejected = 0;
    while t < 1.0 {
        if t + h > 1.0 { h = 1.0 - t; }
        let (y_new, err) = rk45_step(t, y, h);
        if err > tol {                     // 拒绝:缩小重试(不推进)
            h *= 0.5; rejected += 1; continue;
        }
        // 接受:h_new = h*(tol/err)^(1/5),指数 1/5 来自 4 阶误差 O(h^5)
        y = y_new; t += h;
        h *= (tol / err.max(1e-30)).powf(0.2).clamp(0.1, 5.0);
        accepted += 1;
    }
    println!("积分结果 y(1) = {:.10}", y);
    println!("解析解   y(1) = {:.10}", std::f64::consts::E);
    println!("绝对误差 = {:.2e}", (y - std::f64::consts::E).abs());
    println!("接受 {} 步,拒绝 {} 步", accepted, rejected);
    assert!((y - std::f64::consts::E).abs() < 1e-8, "嵌入式 RK45 应达高精度");
    println!("\n校验通过:5 阶解达到 1e-8 精度");
}
```

### 3.4 GPU 自适应的陷阱与破解

自适应积分在 CPU 上平平无奇，但在 GPU 上却暗藏杀机。GPU 着色器没有递归，循环边界有严格限制，而且**绝不能让任何像素的死循环拖垮整个 warp**。

`march` 函数的自适应循环有三个精心设计的语义：

**语义一：预算只消耗"接受"的步。**

```wgsl
var budget = steps_max;     // 硬上限（桌面 300）
loop {
    if (budget == 0u) { break; }
    let step = rk45_step(pos, d, dt);
    let err = step.err;
    if (err > tol * 10.0 && dt > dt_min) {
        dt = dt_min; continue;   // 拒绝：缩小 dt 重试（不消耗预算）
    }
    budget = budget - 1u;        // 接受：消耗一个预算单位
    ...
}
```

被拒绝的步（误差过大）缩小 `dt` 后重试，**不**消耗预算。这保证了"步数"参数真实反映了几何分辨率，而不是"尝试次数"。

**语义二：`dt_min` 是强制接受的底板。**

如果一条光线进入了某区域，那里的误差即使在最短步长下也超容差（极端自旋、视界附近），纯自适应循环会永远 `continue` 下去——GPU 挂起。`dt > dt_min` 这个守卫确保：一旦 `dt` 已经触底，**无论误差多大都接受这一步**：

```wgsl
if (err > tol * 10.0 && dt > dt_min) {  // ← 只有还能缩时才重试
    dt = dt_min; continue;
}
budget = budget - 1u;                    // 否则强制接受
```

这是一个防 GPU 挂起的安全阀。

**语义三：步长按经典公式细化。** 见 3.3 节。

这三个语义在 CPU 镜像 `is_captured_rk45` 里被逐字复制，并由 `tests/physics_test.rs` 的循环级测试守护——测试不仅验证单步，更验证预算、`dt_min` 底板、`r_+(\chi)` 捕获半径这些**循环级不变量**。

### 3.5 一个诚实的命名：RK45 是表，不是阶

代码注释里有一段罕见的诚实声明：

> the per-stage `normalize(dir + ...)` projection (faithful to the shader) makes the *realized* error estimate shrink between 2nd and 4th order in dt depending on geometry, not a clean 5th order.

积分器在每个阶段后都对方向**重新归一化**（`normalize`）。这保持方向为单位向量（物理上，光线的切向量始终单位长），但破坏了纯 Dormand-Prince 表的严格误差阶。实测的误差衰减介于 2\~4 阶之间，取决于几何。

作者没有粉饰这一点。自适应循环**只依赖误差关于 `dt` 单调**这一性质（`rk45_step_error_shrinks_monotonically_with_dt` 测试验证了它），而单调性成立。"RK45"命名遵循着色器的 Butcher 表，而非精度保证。这种对自身局限的坦诚，是工程成熟度的标志。

---

## 四、架构的支点：CPU↔GPU 物理镜像

整个项目最独到的架构决策，是**将物理积分器在两个地方维护两份等价实现**：


|     | CPU（`src/physics.rs`）              | GPU（`black_hole.wgsl`） |
| --- | ---------------------------------- | ---------------------- |
| 语言  | Rust                               | WGSL                   |
| 用途  | **单元测试**                           | **实时渲染**               |
| 单步  | `kerr_bending_accel` / `rk45_step` | `deriv` / `rk45_step`  |
| 循环  | `is_captured_rk45`                 | `march` 的 `loop`       |
| 可测  | `cargo test`                       | 着色器无法单测                |


**为什么要做镜像？** 因为 GPU 着色器无法单元测试。整个渲染器的物理正确性——捕获与逃逸的边界、自旋退化、ISCO 收缩——全部系于一段无法被 `cargo test` 触达的 WGSL 代码。如果物理只在着色器里，永远无法在 CI 里回归验证它。

镜像的设计哲学是：**让物理可测的部分尽可能贴近不可测的部分**。`physics.rs` 不是为了运行时性能（二进制几乎只用它的 `kerr_isco`/`kerr_horizon` 做 UI 读数），而是为了**让捕获/逃逸边界这个最关键的物理不变量变得可测试**。

代价是**双份维护**：改变一处物理，必须同步另一处。`AGENTS.md` 明确列出了必须锁步的函数清单（`bending_accel` / `kerr_bending_accel` / `rk45_step` / `is_captured` / `is_captured_rk45`）和三个循环级不变量（预算=仅接受步、`dt_min` 强制接受底板、`r_+(\chi)` 捕获半径）。Butcher 表的系数在 Rust 与 WGSL 两侧逐字一致。

这构成了一种**人肉实现的同伦检查**：两边做的事在语义上相同，测试验证 CPU 侧的语义，注释和纪律保证 GPU 侧跟随。脆弱吗？是。但目前没有更好的办法让 GPU 物理可回归。

---

## 五、渲染管线：七台相机的 HDR 合唱

物理积分只是第一幕。一个"物理正确但视觉平庸"的黑洞——纯黑圆盘加笔直光线——并不动人。singularity-rs 的第二幕是一条**多阶段 HDR 后处理管线**，把物理输出锻造成电影级画面。

### 5.1 全屏 quad 链

整个画面由**七个 `Camera2d`** 协作产出，按 `Camera.order` 从 −20（最先）到 composite（最后）严格排序：

```
offscreen quad ──→ Rgba16Float Image (HDR 场景)
    │
    ▼ order -19
brightpass  ──→ 半分辨率 HDR (亮度提取)
    │
    ▼ order -18 → -17
blur 下采样 ×2  (1/4 → 1/8 分辨率)
    │
    ▼ order -16 → -15
blur 上采样 ×2  (1/8 → 1/4 → 1/2 分辨率)
    │
    ▼ composite
composite   ──→ 窗口 LDR surface (ACES 色调映射)
```

每个阶段都是一个全屏 quad，挂载一个自定义 `Material2d`，读写 `Rgba16Float`（每通道 16 位浮点）的离屏纹理。

**为什么是 16 位浮点？** 这是 HDR 的关键。普通的 8-bit sRGB 纹理每个通道只能存 $[0,1]$。但吸积盘内缘的辐射亮度可以远超 1.0（物理上，热等离子体的辐射强度没有上限），只有浮点目标能保留这个动态范围，供后续 bloom 提取和色调映射使用。`Rgba16Float` 的半精度（6 位指数 + 10 位尾数）足以覆盖渲染所需的亮度量级，又比 32 位浮点省一半显存。

### 5.2 Bloom：金字塔模糊

Bloom（辉光）模拟真实镜头/相机传感器对强光的溢出响应——亮的东西会"晕开"一圈光。管线采用经典的**降采样-上采样金字塔**。

**Bright-pass 亮度提取。** 从 HDR 场景中提取亮度高于阈值的像素。这里用了**软膝**（soft knee）而非硬截断：

```wgsl
let lum = dot(hdr, vec3(0.2126, 0.7152, 0.0722));   // Rec.709 亮度
let soft = max(lum - u.threshold, 0.0) / (lum + 0.0001);
let contribution = hdr * soft;
```

Rec.709 亮度公式 $Y = 0.2126R + 0.7152G + 0.0722B$ 反映人眼对绿光更敏感的特性。软膝让近阈值的像素平滑过渡到零（而非硬切），避免提取出硬边。

**金字塔模糊。** 两次降采样（半→四分之一→八分之一分辨率）+ 两次上采样（八分之一→四分之一→二分之一）。每次卷积用 13 抽头的加权高斯近似核。为什么用金字塔而不是单次大模糊？因为**低分辨率等价于大空间范围的模糊，但成本极低**——在 1/8 分辨率上做 13 抽头卷积，等效于全分辨率上做约 100 抽头的大核卷积，但只花 1/64 的像素数。

上采样阶段以 `blend` 系数（0.6、0.8）与上一级混合，重建宽频率范围的辉光。最终 `bloom_final` 纹理在 composite 阶段叠加到场景上。

一个务实的简化：`BloomQuality` 枚举有 Off/Low/Medium/High 四档，但 `spawn_bloom_pipeline` 实际上**总是发射完整的三级金字塔**（High）。调用方仅以"是否调用"来区分 Off 与 On——部分金字塔留作未来增强。这是"先做对，再做好"的典型工程克制。

### 5.3 ACES 色调映射：从 HDR 到 LDR 的艺术

HDR 场景（亮度可达数十、数百）必须压缩到显示器能表达的 $[0,1]$ LDR 范围。直接线性缩放会让暗部全黑、亮部全白，丢失细节。色调映射曲线解决"如何优雅地压缩动态范围"。

singularity-rs 用 **ACES（Academy Color Encoding System）的 Narkowicz 解析拟合**：

$$f(x) = \frac{x(2.51x + 0.03)}{x(2.43x + 0.59) + 0.14}$$

```wgsl
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x*(a*x+b)) / (x*(c*x+d)+e), vec3(0.0), vec3(1.0));
}
```

这条曲线的形状值得品味：

- $x \to 0$（暗部）：$f(x) \approx bx/e = (0.03/0.14)x \approx 0.21x$——近似线性，但斜率小于 1，**抬高了暗部**，让暗处细节可见；
- $x \to \infty$（亮部）：$f(x) \to a/c = 2.51/2.43 \approx 1.03$——渐近于 1，形成**柔和的胶片肩部**（filmic shoulder），高光柔和滚落而非硬削顶；
- 中间段是平滑的 S 形过渡。

仅 5 次运算，却赋予了画面电影感的"胶片质感"。这是整个视觉"高级感"的来源之一。Composite 公式为：

$$\text{output} = \text{ACES}\!\Big((\text{scene} + \text{bloom}\cdot\text{strength}) \cdot \text{exposure}\Big)$$

> 试运行:观察 ACES 曲线如何把 HDR 亮度 $[0,\infty)$ 压缩到 LDR $[0,1]$——暗部被抬亮,亮部柔和收敛不削顶。

```rust runnable
/// ACES Narkowicz 色调映射:把 HDR 亮度 [0,inf) 压缩到 LDR [0,1]。
fn aces_tonemap(x: f32) -> f32 {
    let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
    ((x * (a*x + b)) / (x * (c*x + d) + e)).clamp(0.0, 1.0)
}

fn main() {
    println!(" HDR 输入    ACES 输出    说明");
    for (x, note) in [(0.0,"纯黑"),(0.1,"暗部"),(0.5,"中灰"),(1.0,"标准白"),(2.0,"明亮"),(5.0,"强光"),(50.0,"极亮")] {
        println!("  {:<8.2}   {:.5}    {}", x, aces_tonemap(x), note);
    }
    let slope = (aces_tonemap(0.02) - aces_tonemap(0.0)) / 0.02;
    println!("\n暗部等效斜率 ≈ {:.3} (压暗部、抬亮细节)", slope);
}
```

---

## 六、吸积盘：从平板到体积的进化

吸积盘是黑洞渲染的视觉主角。它的难点在于：盘是一个**三维的、有内部结构的、旋转的等离子体**，而光线在盘内穿行时步长是自适应变化的。

### 6.1 体积积分与"辐条"bug

早期的实现用每步采样的密度乘以 RK45 步长来积分盘的不透明度：

$$\alpha_{\text{step}} = \text{density}(\vec{p}) \times \text{step\_len}$$

这产生了一个诡异的视觉 bug：**盘上出现径向辐条**。

根因精妙。RK45 的步长是**空间自适应**的——黑洞附近步长小（`dt_min` 触底），远处步长大（`dt_max`），且相邻像素的步长网格不同。用 `density × step_len` 加权，等于把一个**每像素不同的亮度调制**沿径向注入——步长最小处（内盘）辐条最密。本质上，积分权重耦合了不该耦合的数值参数。

修复方案 `integrate_disk_segment` 展现了出色的工程洞察：**将每步解析地裁剪到盘的几何板层（slab），按板层内的世界空间交线长度加权，而非按 RK45 步长**。

数学上，设盘是 $|y| \le H(r)$ 的板层（$H$ 是随半径变化的高度）。光线的这一步从 $\vec{p}_{\text{prev}}$ 到 $\vec{p}_{\text{new}}$，参数化为 $\vec{p}(t) = \vec{p}_{\text{prev}} + t(\vec{p}_{\text{new}} - \vec{p}_{\text{prev}})$，$t\in[0,1]$。解 $|y(t)| = H$ 得到进入/离开板层的参数 $t_0, t_1$，于是光线**在盘内的几何长度**为：

$$\text{seg\_len} = (t_1 - t_0)\,|\vec{p}_{\text{new}} - \vec{p}_{\text{prev}}|$$

```wgsl
let seg_len = (t1 - t0) * length(new_pos - prev);
for (var i = 0u; i < N; i++) {
    let s = disk_color_volumetric(p, dir);
    let ds = s.density * seg_len / f32(N);  // 权重 ∝ 几何长度，非 RK45 dt
    ...
}
```

`seg_len` 只取决于光线入射角和当地板层高度，**与积分步长完全解耦**。辐条消失。

### 6.2 随机抖动消灭摩尔纹

即使修了辐条，盘上仍出现**同心摩尔环**。N 个采样点落在确定性的 $(i+0.5)/N$ 网格上，与二维像素网格、与每条光线的 RK45 步格发生混叠——这是经典的采样混叠。

修复是**分层随机抖动（stratified jittered sampling）**。在第 $i$ 个分层（stratum，区间 $[i/N, (i+1)/N]$）内，采样位置加上一个该分层内的均匀随机偏移：

$$t_i = t_0 + (t_1 - t_0)\cdot\frac{i + \xi_i}{N}, \qquad \xi_i \in [0,1)$$

```wgsl
let stratum = f32(i) + hash13(vec3<f32>(pixel_seed, f32(i)));
let t = t0 + (t1 - t0) * stratum / f32(N);
```

关键性质：抖动幅度限制在分层内（$\le 1/N$），所以**积分阶不变**（仍是 $N$ 点求积），期望的积分值无偏。但确定性网格的相干摩尔带变成了非相干噪声，而 HDR + bloom 管线对非相干噪声的容忍度远高于相干带。

种子**只**用像素坐标（不含时间），于是噪声是空间稳定的——没有逐帧闪烁。这是两个层次的抗混叠智慧：物理积分层（解耦步长）和采样层（随机化网格），各自针对一种混叠机制。

### 6.3 黑体色与 Novikov-Thorne 温度

盘的颜色有两种模式。**梯度模式**是手工调的白热→深橙渐变加牛顿多普勒。**黑体模式**则走物理路线，由三部分组成。

**(1) Novikov-Thorne 径向温度剖面。** 标准薄吸积盘（Shakura-Sunyaev / Novikov-Thorne 模型）的局部有效温度随半径的分布为：

$$T(r) \propto \left(\frac{r_{\text{ISCO}}}{r}\right)^{3/4}\left(1 - \sqrt{\frac{r_{\text{ISCO}}}{r}}\right)^{1/4}$$

物理直觉：内缘（$r \to r_{\text{ISCO}}$）物质势能释放最剧烈，温度最高；外缘温度衰减。代码用 `radial_temp` 实现这一剖面：

```wgsl
let isco_r = clamp(inner / r, 0.0, 1.0);
let nt_factor = max(0.0, 1.0 - sqrt(isco_r));
let radial_temp = pow(isco_r, 0.75) * pow(nt_factor, 0.25);
```

**(2) Tanner-Helland 黑体色温映射。** 给定温度 $T$（开尔文），黑体辐射的峰值波长由维恩位移定律 $\lambda_{\max} = b/T$ 决定（$b \approx 2.898\times10^{-3}\,\text{m·K}$）。高温→短波→偏蓝；低温→长波→偏红。Tanner-Helland 给出了一个分段拟合，直接从 $T$ 算出 sRGB：

```wgsl
// 简化：T≤6600K 时 R=255，G/B 用对数拟合；T>6600K 时 B=255，R/G 用幂律拟合
let t = max(temp, 1.0) / 100.0;   // 归一化
```

6500 K 基准温度让内盘呈暖白色（接近 D65 标准白光），向外渐变为深橙——这正是 Gargantua 的标志性外观。注意输出要再转线性（`pow(srgb, 2.2)`）才能进 HDR 管线。

**(3) Kerr 四速度多普勒因子。** 这是黑体模式相对梯度模式最物理的地方。盘物质沿赤道圆轨道运动，其四速度 $u^\mu$ 由 Kerr 度规的赤道分量决定。归一化条件 $g_{\mu\nu}u^\mu u^\nu = -1$ 给出时间分量 $u^t$，光子的能量比（观测者/发射者）为：

$$\delta = \frac{E_{\text{obs}}}{E_{\text{em}}} = \frac{1}{g_{\mu\nu}u^\mu k^\nu}$$

其中 $k^\nu$ 是光子四波矢。代码里 `kerr_doppler` 实现了完整的赤道 Kerr 度规分量 $g_{tt}, g_{t\phi}, g_{\phi\phi}$ 求解：

```wgsl
let g_tt = -(1.0 - 2.0*m/r);
let g_tphi = -2.0*m*a/r;
let g_phiphi = r*r + a*a + 2.0*m*a*a/r;
let u_t_sq = -(g_tt + 2.0*omega*g_tphi + omega*omega*g_phiphi);
let u_t = 1.0 / sqrt(max(1e-6, u_t_sq));
let l_photon = pos.z*dir.x - pos.x*dir.z;   // 光子角动量
let delta = 1.0 / max(0.01, u_t*(1.0 - omega*l_photon));
```

其中 $\Omega = d\phi/dt$ 是开普勒角速度（见第七节）。这样得到的 $\delta$ 乘到温度上（$\delta$ 决定观测到的温度偏移），再用 $\delta^{3.5}$ 作 beaming（见 6.4）。**接近侧蓝移变亮变蓝，远离侧红移变暗变红**——这是梯度模式无法表达的物理特征。

### 6.4 相对论 beaming 的通用形式

盘的亮度和喷流的亮度都用到相对论 beaming。一个以速度 $\beta = v/c$ 运动、方向 $\hat{n}$（指向观测者）的各向同性辐射源，观测到的辐射功率增强因子（多普勒增亮）为：

$$\delta = \frac{1}{\gamma(1 - \boldsymbol{\beta}\cdot\hat{n})}$$

其中洛伦兹因子 $\gamma = 1/\sqrt{1-\beta^2}$。辐射强度按 $\delta^p$ 增亮，幂指数 $p$ 取决于辐射机制：连续谱 $p=3$，某种几何平均取 $p=3.5$。

代码里盘用 $\delta^{3.5}$，喷流用 $\delta^{3.5}$（并钳制上限 8×）。喷流速度 $\beta = 0.92$（接近光速的外流），这意味着接近的喷流那侧极其明亮、远离的那侧极暗——双极不对称。

> 试运行:$\beta=0.92$ 的喷流在不同视线角下的多普勒增亮——接近侧增亮数百倍,远离侧几乎不可见。

```rust runnable
/// 相对论多普勒增亮因子 δ = 1/(γ(1−β·cosθ)),辐射强度 ∝ δ^p。
fn doppler_factor(beta: f64, cos_theta: f64) -> f64 {
    let gamma = 1.0 / (1.0 - beta*beta).sqrt();
    1.0 / (gamma * (1.0 - beta * cos_theta))
}

fn main() {
    let beta = 0.92;
    println!("beta = {} (喷流外流速度)\n", beta);
    println!(" cosθ    delta      delta^3.5       说明");
    for (ct, note) in [(1.0,"正对(接近)"),(0.5,"侧向接近"),(0.0,"垂直"),(-0.5,"侧向远离"),(-1.0,"背离")] {
        let delta = doppler_factor(beta, ct);
        let beaming = delta.powf(3.5);
        println!(" {:>5.1}   {:>6.3}    {:>12.3e}    {}", ct, delta, beaming, note);
    }
    let ratio = doppler_factor(beta, 1.0).powf(3.5) / doppler_factor(beta, -1.0).powf(3.5);
    println!("\n接近/背离 亮度比 ≈ {:.0}:1", ratio);
}
```

### 6.5 相对论喷流：自旋驱动的极轴喷流

沿自旋轴（$+Y$）的双极喷流，关键的自洽性门控：

```wgsl
// 相对论喷流是自旋驱动的（Blandford-Znajek）：机制汲取能层，
// 而能层只在旋转黑洞存在。χ≈0 时没有驱动力。
if (uniforms.spin < 0.05) { return; }
```

Blandford-Znajek (1977) 机制表明，喷流的功率正比于黑洞自旋的平方 $P_{\text{jet}} \propto \chi^2$。即便 UI 开关 `jets_enabled` 为真，自旋为零时喷流也被物理地抑制——**开关表达用户意图，自旋表达物理**。没有这个门控，默认场景（spin=0）会在极轴上方出现毫无物理成因的蓝白光柱。

喷流的形态用高斯径向衰减 $\exp(-r^2/\sigma^2)$ 和指数长度衰减 $\exp(-y \cdot 0.05)$，加上沿轴向流动的湍流噪声（`flow = pos.y*2 - time*8`，让上下喷流分别向外流）。

---

## 七、轨道力学：Kerr 圆轨与 Lense-Thirring 进动

行星（如果开启）沿 Kerr 赤道圆轨道运行，并受 Lense-Thirring 效应发生轨道面进动。这给场景注入了动态的生命力。

### 7.1 开普勒角速度

赤道顺行圆轨道的角速度，Bardeen (1972) 给出：

$$\Omega_\phi = \frac{\sqrt{M}}{r^{3/2} + a\sqrt{M}}$$

```rust
pub fn kerr_orbital_frequency(r: f32, chi: f32) -> f32 {
    let a = chi * 0.5;
    1.0 / (r.powf(1.5) + a)   // M=0.5 时 √M=1/√2，此处做了单位归并
}
```

$\chi = 0$ 时 $a=0$，$\Omega_\phi = M^{1/2}/r^{3/2} = \sqrt{M}/r^{3/2}$，正是牛顿开普勒第三定律的形式 $\Omega \propto r^{-3/2}$——又一个精确退化。$a>0$（顺行）时分母变大，角速度比牛顿情形**更小**：自旋"帮"物质转，所以同样的半径转得更从容。

### 7.2 Lense-Thirring 节点进动

旋转黑洞拖曳时空，使得偏离赤道的轨道面会绕自旋轴缓慢进动——这就是 Lense-Thirring 效应。进动率 $\Omega_{\text{LT}} = \Omega_\phi - \Omega_\theta$，其中 $\Omega_\theta$ 是垂直 epicyclic 频率（Okazaki 1987; Kato-Fukue-Mineshige）：

$$\Omega_\theta^2 = \Omega_\phi^2\left(1 - \frac{4a\sqrt{M}}{r^{3/2}} + \frac{3a^2}{r^2}\right)$$

```rust
let ratio = (1.0 - 4.0*a*m.sqrt()/r.powf(1.5) + 3.0*a*a/(r*r)).max(0.0);
let omega_theta = omega_phi * ratio.sqrt();
omega_phi - omega_theta
```

注释里特别强调了交叉项是 $a\sqrt{M}/r^{3/2}$（半径 $-1.5$ 次幂），**不是** $a\cdot\Omega_\phi/r$——后者会让 $\Omega_\theta$ 偏大、进动偏小，且破坏"进动随 $\chi$ 单调增"的物理性质。这是一个容易写错、且错了不报错但物理不自洽的地方，注释的存在就是为了防止未来改坏。

$\chi = 0$ 时括号 $=1$，$\Omega_\theta = \Omega_\phi$，进动为零——史瓦西球对称，无进动。测试 `nodal_precession_strong_field_exceeds_weak_field` 还验证了强场区（$r<6$）精确进动率**超过**弱场近似 $2aM/r^3$：弱场近似只保留主阶，强场下高阶项变得重要。

由于真实进动极慢，UI 提供 `planet_time_scale`（默认 50×）放大模拟时间，否则进动在视觉时间尺度上不可见。

> 试运行:随自旋 $\chi$ 增大,赤道圆轨角速度如何变化、Lense-Thirring 进动如何从零涌现。

```rust runnable
const M: f32 = 0.5;

/// Kerr 赤道顺行圆轨道角速度。chi=0 退化为牛顿 Ω = √M / r^1.5。
fn kerr_orbital_frequency(r: f32, chi: f32) -> f32 {
    let a = chi * M;
    M.sqrt() / (r.powf(1.5) + a * M.sqrt())
}

/// Lense-Thirring 节点进动率 Ω_LT = Ω_φ − Ω_θ。chi=0 为零。
fn kerr_nodal_precession(r: f32, chi: f32) -> f32 {
    let a = chi * M;
    let omega_phi = kerr_orbital_frequency(r, chi);
    let ratio = (1.0 - 4.0*a*M.sqrt()/r.powf(1.5) + 3.0*a*a/(r*r)).max(0.0);
    let omega_theta = omega_phi * ratio.sqrt();
    omega_phi - omega_theta
}

fn main() {
    let r = 6.0;
    println!("半径 r = {} Rs:\n", r);
    println!(" chi    Ω_φ          Ω_LT(进动)");
    for chi in [0.0_f32, 0.3, 0.6, 0.9] {
        let omega = kerr_orbital_frequency(r, chi);
        let prec = kerr_nodal_precession(r, chi);
        println!(" {:.1}    {:.6}     {:.6}", chi, omega, prec);
    }
    assert!(kerr_nodal_precession(r, 0.0).abs() < 1e-6, "chi=0 进动应为零");
    assert!(kerr_nodal_precession(r, 0.9) > kerr_nodal_precession(r, 0.3), "进动应随 chi 增");
    println!("\n校验通过:chi=0 零进动(球对称),进动随自旋单调增长");
}
```

---

## 八、Bevy 0.19 的暗礁：五个会致静默失败的陷阱

这个项目最"接地气"的工程价值，在于它记录并解决了 Bevy 0.19 上一系列**静默失败**——不报错、不崩溃，只是画面空白或灰屏。这些 trap 的诊断过程本身就是宝贵的工程知识。

### 8.1 `nudge_camera`：静止相机不渲染

Bevy 0.19 issue #24448：静止的 `Camera2d` 在首帧后停止渲染。离屏相机是全场最静止的实体，它一旦冻结，合成相机会每帧重采样一张**陈旧纹理**——画面冻住。

修复极简而精妙：每帧给相机一个**亚像素级的正弦抖动**：

```rust
let nudge = (time.elapsed_secs() * 5.0).sin() * 1e-3;
t.translation.x = nudge;
```

$10^{-3}$ 的平移远低于一个像素（世界单位下），视觉不可见，却让视图矩阵每帧变化，render graph 持续产出帧。振幅 $A\sin(\omega t)$ 的选择：$\omega=5$ 让运动平滑，$A=10^{-3}$ 确保亚像素。这个 workaround 被明确标注"上游修复后移除"。

### 8.2 egui 上下文必须钉在合成相机上

bevy_egui 0.41 会把 `PrimaryEguiContext` 自动赋给**第一个生成的相机**——也就是离屏相机。但离屏相机渲染到 `Rgba16Float` 纹理，而 egui 的管线是 `Rgba8UnormSrgb`——**格式不匹配崩溃**。

修复链：`disable_egui_auto_context`（在 `PreStartup` 关掉自动赋值）+ 显式把 `PrimaryEguiContext` 组件加到合成（窗口）相机上。现在 bloom 管线有 7 个相机，这个陷阱尤其致命。

### 8.3 行星存储缓冲必须是真实的 `ShaderBuffer` 资产

`Handle::default()`（空句柄）会让 `AsBindGroup` 每帧返回 `RetryNextUpdate`，**静默跳过** quad 的绘制——画面只剩相机的清除色（灰）。修复：启动时预填一个 `MAX_PLANETS`（32）大小的零缓冲，`upload_planets` 原地修改它，句柄永不变。

更微妙的是：**绝不能每帧分配新的 `ShaderBuffer`**。新资产还没有 GPU 资产，绑定解析又返回 `RetryNextUpdate`。必须原地修改已存在的资产，让 `GpuShaderBuffer::prepare_asset` 复用同一个 GPU 缓冲。

### 8.4 天空盒纹理绑定必须声明 `dimension = "cube"`

`AsBindGroup` 派生默认是 2D 纹理，但着色器声明的是 `texture_cube<f32>`。布局不匹配使管线特化失败，quad 静默不绘制。

### 8.5 WebGPU 上必须用 `textureSampleLevel`

天体盒在 RK45 主循环内被采样，而该循环控制流不均匀（`if (accum_alpha > 0.99) { break; }`，每像素提前退出）。WGSL 规范禁止 `textureSample` 出现在非均匀控制流（它需要屏幕空间导数做 mipmap 选择）。Chrome 的 Tint 强制执行并拒绝编译，而桌面端 naga 不执行——**只在 Web 构建崩溃**。`textureSampleLevel` 接受显式 LOD，允许在非均匀流中使用。

---

## 九、设计哲学的几条主线

通读全文，可以提炼出几条贯穿项目的设计原则。

### 9.1 单一物理数据源

`disk_inner` 被 ISCO 覆盖；捕获半径用 $r_+(\chi)$ 而非硬编码；喷流被自旋门控。凡物理能决定的，不让用户/代码任意设。这消除了"看起来对但物理不自洽"的整类状态。

### 9.2 锁步四处的镜像纪律

镜像一个新参数到端到端，必须同步四处：`BlackHoleParams`（`params.rs`）→ `BlackHoleUniforms`（`material.rs`，注意 WGSL `vec3` 对齐填充）→ `mirror_params` 赋值（`plugin.rs`）→ 着色器结构体及使用。质量枚举还要加 UI 入口。这条纪律是物理正确性的程序性保障。

### 9.3 质量分级：桌面与 Web 的双轨

每个质量参数通过 `cfg!(target_arch = "wasm32")` 给出两套默认：`steps` 200/300、`render_scale` 0.5/0.75、bloom Low/High、disk Low/High、AA Off/Low。这不是简单的"Web 弱一点"，而是**对每种质量维度独立权衡**——RK45 积分已经是固定步长 RK4 的约 10 倍开销，所以 Web 的 `render_scale` 直接砍到 0.5 换取交互性。

### 9.4 调试设施内建，但克制

`tests/physics_test.rs` 守护物理不变量；`preset_test.rs` 守护参数预设；着色器内联所有函数（因为 naga_oil 跨模块导入曾静默失效）；注释详尽记录每个 workaround 的因果与上游 issue 编号。这些都是"为六个月后的自己/下一位维护者"写的。

### 9.5 资产嵌入：发布即自洽

Release 构建通过 `bevy_embedded_assets`（`PluginMode::ReplaceAndFallback`）把整个 `assets/` 树嵌入二进制——发布的 `singularity-rs` 可执行文件是**完全自洽**的，旁边不需要 `assets/` 文件夹。Debug 构建故意跳过嵌入：编辑 `.wgsl` 后 `cargo run` 立即反映，无需重编译 Rust。这个 debug/release 的区分，是渲染器开发循环的效率关键。

---

## 十、未竟之路：Phase 4 与精确 Kerr

当前实现用的是一个**近似的 Kerr 弯曲加速度**（径向 Schwarzschild 项 + Lense-Thirring 前导拖曳项），而非 Kerr 时空的精确 Cartesian 伪哈密顿量。精确形式涉及 Kerr-Schild 坐标下的伪哈密顿量，用 $\Sigma$、$\Delta$ 等度量函数表示：

$$\Delta = r^2 - 2Mr + a^2, \qquad \Sigma = r^2 + a^2\cos^2\theta$$

精确方法还用 Carter (1968) 发现的**第四守恒量**实现 Hamilton-Jacobi 可分离性，能得到亚百分比精度的光子轨道。但在高自旋的 photon sphere 附近，近似方法的精度会下降——自旋依赖的捕获半径 $r_+(\chi)$ 是精确的，但光线轨迹的偏折细节是近似的。

`phase4-exact-kerr-experimental` 分支尝试过精确 Kerr-Schild 哈密顿量：CPU 侧达到了亚百分比的 $b_{\text{crit}}$ 精度（$\chi=0$: 0.08%，$\chi=0.9$: 1.5%），但 GPU 镜像存在未解决的 CPU↔WGSL 数值发散——渲染的盘没有引力透镜，尽管 CPU 追踪显示正确的 173° 偏折。

这是一个悬而未决的硬骨头：在 GPU 浮点精度下，精确 Kerr 哈密顿量的数值稳定性如何保证？两条等价实现为何发散？这或许是这个项目最深邃的开放问题。

---

## 结语：物理与工程的合奏

做这个渲染器的过程中，越来越确信一件事：物理保真与工程约束不是对立的取舍，而是可以互相成就的。

- **物理保真**（自适应积分、Kerr 退化、ISCO、多普勒）通过 CPU 镜像变得**可测试**；
- **工程约束**（GPU 不能挂起、WebGPU 规范、Bevy 0.19 的 bug）通过精心设计的语义（预算、底板、nudge）被**驯服**；
- **视觉目标**（Gargantua）通过 HDR 管线、ACES、体积盘、抗混叠从物理中**自然涌现**，而非贴图伪造。

每一个像素都是一次从相机出发、穿越弯曲时空的微型宇宙学实验。六万亿次这样的实验每秒在 GPU 上发生。这就是实时黑洞渲染的诗意所在——用工程的严谨，让广义相对论的方程在眼前活过来。

---

### 附录：关键公式速查


| 物理量               | 公式                                                                               | 代码位置                          |
| ----------------- | -------------------------------------------------------------------------------- | ----------------------------- |
| 史瓦西半径（单位制）        | $R_s=1,\; M=0.5$                                                                 | 全局约定                          |
| 临界撞击参数            | $b_{\text{crit}}=\tfrac{3\sqrt3}{2}R_s\approx2.598$                              | `BCRIT`                       |
| 径向弯曲加速度           | $\vec a=-\tfrac32 R_s\tfrac{h^2}{r^5}\vec p$                                     | `bending_accel`               |
| Lense-Thirring 拖曳 | $\tfrac{2Ma}{r^3}(\hat a\times\vec d)$                                           | `kerr_bending_accel`          |
| Kerr 视界           | $r_+=M+\sqrt{M^2-a^2}$                                                           | `kerr_horizon`                |
| Kerr ISCO         | Bardeen-Press-Teukolsky 闭式                                                       | `kerr_isco`                   |
| RK45 步长控制         | $h_{\text{new}}=h(\tfrac{\text{tol}}{\text{err}})^{1/5}$                         | `is_captured_rk45`            |
| ACES 色调映射         | $\tfrac{x(2.51x+0.03)}{x(2.43x+0.59)+0.14}$                                      | `composite.wgsl`              |
| 多普勒增亮             | $\delta=\tfrac1{\gamma(1-\boldsymbol\beta\cdot\hat n)}$，强度 $\propto\delta^{3.5}$ | `apply_doppler`/`sample_jets` |
| 开普勒角速度            | $\Omega_\phi=\tfrac{\sqrt M}{r^{3/2}+a\sqrt M}$                                  | `kerr_orbital_frequency`      |
| Lense-Thirring 进动 | $\Omega_{\text{LT}}=\Omega_\phi-\Omega_\theta$                                   | `kerr_nodal_precession`       |


---

*本文基于 singularity-rs v0.2.0 源码（`src/physics.rs`、`assets/shaders/black_hole.wgsl`、`src/render/`）撰写。所有代码引用可在仓库中定位。*