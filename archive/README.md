# archive

不是产品。放这里是为了以后还能对照，不是给 `./build.sh` 用的。

| 目录 | 原来是什么 | 为什么搬出来 |
|------|------------|--------------|
| `webview-ui/` | `app/ui` HTML/JS GUI | mac 已经换成 Qt。留在活树上会变成第二份 GUI 和两套翻译。 |
| `windows-pack/` | npm Tauri CLI、远程编 Windows 壳的脚本 | 这轮只留 mac 开发。Windows GUI 不算产品，本机 `./build.sh` 也不再交叉编 Windows Core。 |

要救回 Windows HTML 壳：把 `webview-ui` 拷回 `app/ui`，`windows-pack` 里的脚本拷回 `script/`，再按当时的 `build.sh` 远程打包。
