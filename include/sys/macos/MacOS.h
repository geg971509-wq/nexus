#pragma once

#include <QProcess>
#include <QString>

// Timeout for QProcess::waitForStarted on the elevated osascript launches
// (core setuid and warp-client up/down). Shared so every elevation path agrees.
inline constexpr int kOsascriptStartTimeoutMs = 15000;

// Optional long-running shell fragment runs after setuid succeeds, still inside the
// same administrator privileges prompt (e.g. elevated warp-client up).
// When postCommand is non-empty and this returns 0, *elevatedProcess receives the
// still-running osascript process (caller owns it). On failure the helper cleans up.
int Mac_Set_Core_Permissions(const QString &path,
                             const QString &postCommand = QString(),
                             QProcess **elevatedProcess = nullptr);
bool Mac_Core_Path_Supports_Setuid(const QString &path);
