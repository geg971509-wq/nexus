# archive

不是产品。放这里是为了以后还能对照，不是给 `./build.sh` 用的。

| 目录 | 原来是什么 | 为什么搬出来 |
|------|------------|--------------|
| `webview-ui/` | `app/ui` HTML/JS GUI | mac 已经换成 Qt。留在活树上会变成第二份 GUI 和两套翻译。 |
| `windows-pack/` | npm Tauri CLI、远程编 Windows 壳的脚本 | 这轮只留 mac 开发。Windows GUI 不算产品，本机 `./build.sh` 也不再交叉编 Windows Core。 |

这里是历史参考，不是可直接恢复的完整旧版构建快照。旧 Windows HTML 壳还依赖当时的
`app/src-tauri`、`app/package.json` 等已经不在当前受版本控制源码中的文件；如需恢复该产品线，
应从对应历史提交重建，而不是把本目录部分文件拷回当前活树。
