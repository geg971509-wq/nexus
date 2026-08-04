#include "include/sys/linux/LinuxCap.h"

#include <QDebug>
#include <QFileInfo>
#include <QProcess>
#include <QStandardPaths>

int Linux_Run_Command(const QString &commandName, const QStringList &args) {
  const auto pkexec = Linux_FindCapProgsExec("pkexec");
  const auto command = Linux_FindCapProgsExec(commandName);
  if (pkexec.isEmpty() || command.isEmpty())
    return -1;
  return QProcess::execute(pkexec, QStringList{command} + args);
}

bool Linux_HavePkexec() {
  const auto pkexec = Linux_FindCapProgsExec("pkexec");
  if (pkexec.isEmpty())
    return false;

  QProcess p;
  p.setProgram(pkexec);
  p.setArguments({"--help"});
  p.setProcessChannelMode(QProcess::SeparateChannels);
  p.start();
  p.waitForFinished(500);
  return (p.exitStatus() == QProcess::NormalExit ? p.exitCode() : -1) == 0;
}

QString Linux_FindCapProgsExec(const QString &name) {
  if (QFileInfo(name).fileName() != name)
    return {};

  static const QStringList trustedPaths{"/usr/bin", "/bin", "/usr/sbin",
                                        "/sbin"};
  const auto exec = QStandardPaths::findExecutable(name, trustedPaths);

  if (exec.isEmpty())
    qDebug() << "Executable" << name << "could not be resolved";
  else
    qDebug() << "Found exec" << name << "at" << exec;

  return exec;
}
