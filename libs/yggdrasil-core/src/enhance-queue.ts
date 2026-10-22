/** 一条待执行的增强调用：全局函数名 + 已求值的参数。 */
export interface EnhanceCall {
  name: string;
  args: unknown[];
}

/**
 * P5 §8.2 增强调用队列（仅类型，无运行时代码）。
 *
 * core 入口无条件建队；`invoke_optional_global`（wasm 端）发现目标函数未定义且队列
 * 存在时入队。enhance / lightbox IIFE 尾部各 flush 一次：只执行已就绪的函数，其余
 * 保留等对应脚本加载。队列存在不代表本页会加载 enhance——没有增强组件的页面不会
 * 产生入队调用，滞留项等价于原先的静默跳过。
 */
