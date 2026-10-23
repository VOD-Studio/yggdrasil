# 前台全局音乐播放器

Yggdrasil 使用精确锁定的 `@violetjs/kite-player@0.3.0` Web Components 入口，不引入 React。
播放器仅挂载在 `FrontendLayout`，位于页面路由与路由加载边界之外：前台 SPA
切页保持播放；进入后台、登录等非前台布局时暂停并移除界面。返回前台可手动继续，
不会自动播放。整页刷新会恢复队列和进度，但同样不会自动播放。

第一阶段只提供全局播放器，不包含正文歌曲卡片、编辑器音乐节点或本站音频上传。

## Yggdrasil 配置

在 **Yggdrasil 服务进程** 的环境变量中设置：

```dotenv
KITE_PLAYER_API_URL=https://music.example.com
KITE_PLAYER_PLAYLISTS=123456,234567
```

- `KITE_PLAYER_API_URL` 是浏览器可访问的公开服务根地址，**不带 `/player/v1`**。
  也可包含反向代理路径前缀，如 `https://blog.example.com/music`，前提是代理将
  `/music/player/v1/*` 转发到 Kite 的 `/player/v1/*`。
- HTTPS 博客必须使用 HTTPS 音乐服务；本地 HTTP 开发可使用 `http://localhost:3723`。
- `KITE_PLAYER_PLAYLISTS` 为逗号分隔的数字歌单 ID，最多 20 个；顺序保留、重复项去重。
  第一个歌单用于首次载入，随后优先保留访客自己的播放队列。
- 任一配置为空时功能关闭：不注入播放器脚本，不请求 Kite 服务。
- 配置启动后固定，修改需重启 Yggdrasil 服务进程；Compose 环境变量变更需要重新创建
  容器，单纯 `compose restart` 不会注入新环境变量。配置无效时记录警告并关闭播放器，
  不影响正文阅读。
- 这两个值经公开 server function 传给浏览器；不得包含管理 token、账号 Cookie
  或其他秘密。API URL 拒绝用户名、密码、查询参数与 URL 片段。
- Docker Compose 的项目 `.env` 默认只是插值来源，不一定自动进入容器。若部署使用
  显式 `environment:`，需要将这两个变量加入 Yggdrasil 服务：

```yaml
services:
  yggdrasil:
    environment:
      KITE_PLAYER_API_URL: ${KITE_PLAYER_API_URL:-}
      KITE_PLAYER_PLAYLISTS: ${KITE_PLAYER_PLAYLISTS:-}
```

## Kite 后端要求

前端连接的是 **公开只读播放器模式** `/player/v1`，不是需要 Bearer token 的
管理 REST/gRPC 接口。不应向浏览器暴露管理端口，也不要在前端代管网易云 Cookie。

在 **kite-server 服务进程** 上至少配置：

```dotenv
KITE_PUBLIC_ADDR=0.0.0.0:3723
KITE_PUBLIC_PLAYLISTS=123456,234567
KITE_PUBLIC_ORIGINS=https://blog.example.com
```

生产环境还应按官方文档配置持久化的 `KITE_PUBLIC_SIGNING_KEY_FILE`、限流及 HTTPS
反向代理。公开服务和管理服务是不同的监听面；反向代理应指向公开服务端口。
只发布实际允许公开的歌单，不要把管理 API 一并暴露。

`KITE_PUBLIC_ORIGINS` 应匹配浏览器访问博客时的真实 Origin（scheme、host、port），
而不是容器内部地址。Kite 的白名单才是歌曲访问控制；Yggdrasil 歌单配置仅控制展示。
反向代理路径前缀部署时，需同时确认返回的流/歌词 URL 被正确路由。

可选服务账号和 `KITE_PUBLIC_SESSION_FILE` 在 Kite 服务端管理。平台条款、版权与公开
播放授权由部署者确认；没有对应权限的曲目仍可能不可播放，不应绕过限制。

上线前验证：

1. 从访问博客的网络访问 `https://music.example.com/player/v1/info`，确认不是管理接口
   的鉴权错误，也不是代理 404。
2. 对允许的歌单请求 `/player/v1/playlists/{id}`，检查实际可播放曲目。
3. 跨域部署检查带博客 `Origin` 的请求是否得到正确的 CORS 响应。
4. 在浏览器手动点击播放，验证播放链接跳转、歌词、暂停和播放进度；不要将短时播放
   URL 固化到数据库或部署配置。

## rua.plus 双机部署

`xun` 与 `rua` 都服务 `https://rua.plus`，因此公开服务只允许这个 Origin；
不需要把两台服务器 IP 加进 CORS。浏览器直连 `https://xunrua.top/player/v1/`，
博客服务本身不转发音频请求，也不需要跨机连接 Kite 管理端口。

两机的 `/root/docker/yggdrasil/.env` 使用相同配置：

```dotenv
KITE_PLAYER_API_URL=https://xunrua.top
KITE_PLAYER_PLAYLISTS=3778678
```

同时将前文的两个环境变量加入各自 `docker-compose.yml` 的 `services.app.environment`。
CI 发布时会重建两台 app；手动应用时，`xun` 使用 `docker compose`，`rua` 使用
`podman compose`，均在该部署目录执行
`--env-file .env up -d --no-deps --force-recreate app`，不重启数据库。

### 独立 Kite 公开服务

`docker/kite-public.container` 是部署在 `xun` 的 rootful Podman Quadlet，
安装到 `/etc/containers/systemd/kite-public.container`，由 `kite-public.service`
管理。它复用已存在的 `kite-auth-token` secret，签名密钥单独保存为
`kite-public-signing-key` secret；首次安装前创建，不能提交密钥内容：

```sh
openssl rand -hex 32 | podman secret create kite-public-signing-key -
systemctl daemon-reload
systemctl start kite-public.service
```

服务固定使用已部署并验证过的 Kite v0.8.0 镜像 digest，只在 `nginx-proxy` 网络提供
3723，不发布宿主机端口；管理监听绑定容器回环。`WantedBy=multi-user.target`
由 Quadlet 在开机时生成启动依赖，无需对生成的 service 执行 `systemctl enable`。
没有配置服务账号，当前按匿名权限播放；歌单中不可播放的付费曲目仍受上游限制。

该实例不属于 Kite 原有管理实例的自动部署事务。升级公开服务时，先验证目标镜像，
更新 Quadlet 的 `Image=` digest 并在宿主拉取，再执行 `systemctl daemon-reload`
和 `systemctl restart kite-public.service`，重新验证歌单、播放、CORS 与白名单。

### HTTPS 反代

`xun` 的 `/root/docker/nginx-proxy/vhost.d/xunrua.top` 增加以下独立路径，
不修改其他站点路由；DNS resolver 是该机现有 Podman `nginx-proxy` 网络的网关：

```nginx
location ^~ /player/v1/ {
    resolver 10.89.0.1 valid=30s ipv6=off;
    set $kite_public http://kite-public:3723;
    proxy_pass $kite_public;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

变量 upstream 配合 DNS 重解析，避免公开容器重建换 IP 后继续访问旧地址。
覆盖而非追加访客传入的 `X-Forwarded-For`，让公开服务只信任这层代理记录的来源。
修改后先 `docker exec nginx-proxy nginx -t`，成功再执行
`docker exec nginx-proxy nginx -s reload`。

上线验收包括：带 `Origin: https://rua.plus` 的 info 与歌单请求返回 200，
响应含匹配的 `Access-Control-Allow-Origin`；其他 Origin 返回 403，
白名单外歌单返回 404；在实际博客页面手动播放并确认进度增长与歌词载入。

## 构建与验证

```sh
make check-dev-tools
make dev-assets
bun run --cwd libs/music-player test
bun scripts/test-music-player.cjs
```

浏览器脚本使用构建后的真实 IIFE 和本地模拟的公开音乐 API，不访问网易云或线上
账号。支持 `PLAYWRIGHT_MODULE`、`CHROMIUM_PATH`，可用 `MUSIC_PLAYER_ARTIFACT_DIR`
保存截图。实际部署的歌单、CORS、账号权限和上游链接仍需上线前单独验证。

包位于 `libs/music-player/`，由 Vite 生成 `public/music-player/player.js` 和 source map；
CSS 已包含在组件 JS 中。`public/music-player/` 是忽略的构建产物。播放器通过现有
browser-library loader 按需加载，不并入全站启动包。

主题跟随页面实际生效的 `html.dark`，不独立追随操作系统；通过 `--kite-*` 变量映射
本站语义颜色和字体。切主题不会重新创建播放器。

`0.3.0` 的音质标签沿用官方组件：服务端返回 `Track.quality` 为 `exhigh`、`lossless`
或 `hires` 时展示，点击可切换高音质与标准音质，保留当前播放进度并记住偏好。
公开流地址须支持 `quality=standard` 参数；没有音质字段的旧服务仍按原有方式播放，
不显示标签。试听曲目的 `trial` 标记和无封面占位也由官方组件处理。

该版本发布未满 24 小时时，通过 `libs/bunfig.toml` 中的按包名豁免安装；工作区与
`bun.lock` 均锁定 `0.3.0`，其他依赖仍遵守原有发布冷却策略。

布局卸载时移除组件、解除主题观察并暂停共享核心；不会调用 Kite 的 `destroy()`，
因为其 `getPlayer()` 保留单例引用，销毁后再次进入前台会拿到已销毁的核心。

## 官方参考

- [集成索引 llms.txt](https://github.com/VOD-Studio/kite/blob/main/llms.txt)
- [公开播放器模式与服务部署](https://github.com/VOD-Studio/kite/blob/main/docs/player.md)
- [播放器组件文档](https://github.com/VOD-Studio/kite/tree/main/packages/kite-player)
