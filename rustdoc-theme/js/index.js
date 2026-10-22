// rustdoc 站点化脚本入口。bun 打成 IIFE（/doc/doc-shell.js），在 after-content 末尾同步加载，
// 先于 rustdoc main.js（defer）执行：能拿到完整静态 DOM，拿不到动态生成的 #rustdoc-modnav。
// 每个功能函数找不到目标元素时直接 return，不抛错。
import { initHeader } from './header.js';
import { localizeHeading } from './heading.js';
import { groupCrateModules, initHashReveal, localizeContent } from './content.js';
import { initToc } from './toc.js';
import { initSidebar } from './sidebar.js';
import { initSearch } from './search.js';

function init() {
  initHeader();
  localizeHeading();
  localizeContent();
  groupCrateModules();
  initHashReveal();
  initToc();
  initSidebar();
  initSearch();
}

init();
