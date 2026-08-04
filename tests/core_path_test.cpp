#include "include/global/CorePath.hpp"

#include <QDir>
#include <QFile>
#include <QTemporaryDir>

#include <cstdlib>

int main() {
  QTemporaryDir dir;
  if (!dir.isValid())
    return EXIT_FAILURE;

  auto corePath = dir.filePath("ThroneCore");
#ifdef Q_OS_WIN
  corePath += ".exe";
#endif
  if (Configs::ResolveCorePath(dir.path()) !=
      QDir::toNativeSeparators(corePath))
    return EXIT_FAILURE;

  QTemporaryDir otherAppDir;
  QTemporaryDir dataDir;
  QTemporaryDir otherDataDir;
  if (!otherAppDir.isValid() || !dataDir.isValid() ||
      !otherDataDir.isValid())
    return EXIT_FAILURE;

  const auto socketId = Configs::CoreSocketId(dir.path(), dataDir.path());
  if (socketId != Configs::CoreSocketId(dir.path(), dataDir.path()) ||
      socketId == Configs::CoreSocketId(dir.path(), otherDataDir.path()) ||
      socketId == Configs::CoreSocketId(otherAppDir.path(), dataDir.path()))
    return EXIT_FAILURE;

#ifndef Q_OS_WIN
  const auto targetPath = dir.filePath("Core Target");
  QFile target(targetPath);
  if (!target.open(QIODevice::WriteOnly) || !target.flush())
    return EXIT_FAILURE;
  target.close();
  if (!QFile::link(targetPath, corePath))
    return EXIT_FAILURE;
  if (Configs::ResolveCorePath(dir.path()) != targetPath)
    return EXIT_FAILURE;
#endif

  return EXIT_SUCCESS;
}
