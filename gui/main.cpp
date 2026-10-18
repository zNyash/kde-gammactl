#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlError>
#include <QStringList>
#include <QUrl>
#include <QVariant>
#include <QVariantList>
#include <cstdlib>

class Backend final : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList outputs READ outputs NOTIFY outputsChanged)
  Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
  Q_PROPERTY(QString message READ message NOTIFY messageChanged)
  Q_PROPERTY(bool messageIsError READ messageIsError NOTIFY messageChanged)

public:
  explicit Backend(QObject *parent = nullptr) : QObject(parent) {}

  QVariantList outputs() const { return m_outputs; }
  bool busy() const { return m_busy; }
  QString message() const { return m_message; }
  bool messageIsError() const { return m_messageIsError; }

  Q_INVOKABLE void refresh() { start({"gui-state"}, Operation::Refresh); }

  Q_INVOKABLE void apply(const QString &output, const QString &curve,
                         double brightness, double contrast, double gamma) {
    QStringList args{"set",
                     "--curve",
                     curve,
                     "--brightness",
                     QString::number(brightness, 'g', 15),
                     "--contrast",
                     QString::number(contrast, 'g', 15),
                     "--gamma",
                     QString::number(gamma, 'g', 15)};
    if (!output.isEmpty())
      args << "--output" << output;
    start(args, Operation::Apply);
  }

  Q_INVOKABLE void reset(const QString &output) {
    QStringList args{"reset"};
    if (!output.isEmpty())
      args << "--output" << output;
    start(args, Operation::Reset);
  }

signals:
  void outputsChanged();
  void busyChanged();
  void messageChanged();
  void operationFinished(bool success, const QString &message);

private:
  enum class Operation { Refresh, Apply, Reset };

  QVariantList m_outputs;
  bool m_busy = false;
  QString m_message;
  bool m_messageIsError = false;

  void setBusy(bool busy) {
    if (m_busy == busy)
      return;
    m_busy = busy;
    emit busyChanged();
  }

  void setMessage(const QString &message, bool isError) {
    m_message = message;
    m_messageIsError = isError;
    emit messageChanged();
  }

  void start(const QStringList &arguments, Operation operation) {
    if (m_busy)
      return;

    const QString program =
        QDir(QCoreApplication::applicationDirPath()).filePath("kde-gammactl");
    if (!QFileInfo::exists(program) || !QFileInfo(program).isExecutable()) {
      const QString error = QString("Cannot find executable %1").arg(program);
      setMessage(error, true);
      if (operation != Operation::Refresh)
        emit operationFinished(false, error);
      return;
    }

    auto *process = new QProcess(this);
    setBusy(true);
    connect(process, &QProcess::errorOccurred, this,
            [this, process, operation](QProcess::ProcessError error) {
              if (error != QProcess::FailedToStart)
                return;
              const QString message =
                  QString("Could not start kde-gammactl: %1")
                      .arg(process->errorString());
              setBusy(false);
              setMessage(message, true);
              if (operation != Operation::Refresh)
                emit operationFinished(false, message);
              process->deleteLater();
            });
    connect(
        process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished),
        this,
        [this, process, operation](int exitCode,
                                   QProcess::ExitStatus exitStatus) {
          const QByteArray standardOutput = process->readAllStandardOutput();
          const QString standardError =
              QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
          const bool success =
              exitStatus == QProcess::NormalExit && exitCode == 0;

          if (operation == Operation::Refresh && success) {
            QJsonParseError parseError;
            const QJsonDocument document =
                QJsonDocument::fromJson(standardOutput, &parseError);
            if (parseError.error != QJsonParseError::NoError ||
                !document.isArray()) {
              setMessage(QString("Could not read monitor information: %1")
                             .arg(parseError.errorString()),
                         true);
            } else {
              QVariantList outputs;
              for (const QJsonValue &value : document.array()) {
                const QJsonObject output = value.toObject();
                if (output.value("enabled").toBool())
                  outputs.append(output.toVariantMap());
              }
              m_outputs = outputs;
              emit outputsChanged();
              setMessage(outputs.isEmpty() ? "No enabled monitors were found."
                                           : QString(),
                         false);
            }
          } else if (!success) {
            const QString message =
                standardError.isEmpty()
                    ? QString("kde-gammactl exited with code %1").arg(exitCode)
                    : standardError;
            setMessage(message, true);
            if (operation != Operation::Refresh)
              emit operationFinished(false, message);
          } else {
            const QString message =
                QString::fromLocal8Bit(standardOutput).trimmed();
            setMessage(message, false);
            emit operationFinished(true, message);
          }

          setBusy(false);
          if (success && operation != Operation::Refresh)
            refresh();
          process->deleteLater();
        });
    process->start(program, arguments);
  }
};

int main(int argc, char *argv[]) {
  QGuiApplication app(argc, argv);
  QCoreApplication::setApplicationName("kde-gammactl");
  QCoreApplication::setOrganizationName("kde-gammactl");

  Backend backend;
  QQmlApplicationEngine engine;
  engine.setInitialProperties(
      {{QStringLiteral("backend"),
        QVariant::fromValue(static_cast<QObject *>(&backend))}});
  const QUrl url(QStringLiteral("qrc:/qt/qml/GammaPanel/Main.qml"));
  QObject::connect(
      &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
      [] { QCoreApplication::exit(EXIT_FAILURE); }, Qt::QueuedConnection);
  QObject::connect(&engine, &QQmlApplicationEngine::warnings, &engine,
                   [](const QList<QQmlError> &warnings) {
                     for (const QQmlError &warning : warnings)
                       qWarning().noquote() << warning.toString();
                   });
  engine.load(url);
  if (engine.rootObjects().isEmpty())
    return EXIT_FAILURE;
  backend.refresh();
  return app.exec();
}

#include "main.moc"
