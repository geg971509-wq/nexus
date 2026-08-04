#pragma once

#include <QCryptographicHash>
#include <QFileInfo>
#include <QString>

namespace Configs {
inline QString ResolveCorePath(const QString &applicationDir) {
  auto path = applicationDir + "/ThroneCore";
#ifdef Q_OS_WIN
  path += ".exe";
#endif
  const QFileInfo info(path);
  if (info.isSymLink())
    path = info.symLinkTarget();
#ifdef Q_OS_WIN
  path.replace("/", "\\");
#endif
  return path;
}

inline QString CoreSocketId(const QString &applicationDir,
                            const QString &dataDir) {
  const auto pathIdentity = [](const QString &path) {
    const QFileInfo info(path);
    const auto canonicalPath = info.canonicalFilePath();
    return canonicalPath.isEmpty() ? info.absoluteFilePath() : canonicalPath;
  };
  QByteArray identity = pathIdentity(applicationDir).toUtf8();
  identity.append('\0');
  identity.append(pathIdentity(dataDir).toUtf8());
  const auto hash = QCryptographicHash::hash(
      identity, QCryptographicHash::Sha256).toHex().first(32);
  return "throneCore-" + QString::fromLatin1(hash);
}
} // namespace Configs
