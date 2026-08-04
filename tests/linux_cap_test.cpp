#include "include/sys/linux/LinuxCap.h"

#include <QFile>
#include <QTemporaryDir>

#include <cstdlib>

namespace {
bool makeExecutable(const QString &path) {
  QFile file(path);
  if (!file.open(QIODevice::WriteOnly | QIODevice::Text))
    return false;
  file.write("#!/bin/sh\nexit 0\n");
  file.close();
  return file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                             QFileDevice::ExeOwner);
}
} // namespace

int main() {
  QTemporaryDir dir;
  if (!dir.isValid())
    return EXIT_FAILURE;

  const QString commandName = "throne-untrusted-test-command";
  const auto fakePkexec = dir.filePath("pkexec");
  const auto fakeCommand = dir.filePath(commandName);
  const auto fakeShell = dir.filePath("sh");
  if (!makeExecutable(fakePkexec) || !makeExecutable(fakeCommand) ||
      !makeExecutable(fakeShell)) {
    return EXIT_FAILURE;
  }

  qputenv("PATH", dir.path().toLocal8Bit());
  const auto trustedShell = Linux_FindCapProgsExec("sh");
  if (trustedShell.isEmpty() || trustedShell == fakeShell ||
      Linux_FindCapProgsExec("pkexec") == fakePkexec ||
      !Linux_FindCapProgsExec(commandName).isEmpty() ||
      !Linux_FindCapProgsExec("/bin/sh").isEmpty()) {
    return EXIT_FAILURE;
  }

  return Linux_Run_Command(commandName, {"path with spaces"}) == -1
             ? EXIT_SUCCESS
             : EXIT_FAILURE;
}
