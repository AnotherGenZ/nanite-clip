#include <QAction>
#include <QDBusConnection>
#include <QDBusError>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QKeySequence>
#include <QList>
#include <QObject>
#include <QString>
#include <QStringConverter>
#include <QTextStream>

#include <KAboutData>
#include <KDBusService>
#include <KGlobalAccel>

namespace {

constexpr int kProtocolVersion = 1;
const auto kPlasmaManualClipHotkeyService = QStringLiteral("plasma_manual_clip_hotkey");

struct HotkeyRequest
{
    int combinedKey = 0;
    QString actionId;
    QString description;
};

void logPhase(const QString &message)
{
    QTextStream err(stderr);
    err.setEncoding(QStringConverter::Utf8);
    err << "[nanite-clip-platform-service] " << message << Qt::endl;
}

void writeMessage(QJsonObject object)
{
    object.insert(QStringLiteral("protocol_version"), kProtocolVersion);

    QTextStream out(stdout);
    out.setEncoding(QStringConverter::Utf8);
    out << QJsonDocument(object).toJson(QJsonDocument::Compact) << Qt::endl;
}

void writeError(const QString &message)
{
    writeMessage(
        QJsonObject{
            {QStringLiteral("event"), QStringLiteral("error")},
            {QStringLiteral("message"), message},
        });
}

QString firstSequenceLabel(const QList<QKeySequence> &sequences)
{
    if (sequences.isEmpty()) {
        return {};
    }

    return sequences.first().toString(QKeySequence::PortableText);
}

void configureApplicationMetadata()
{
    QGuiApplication::setApplicationName(QStringLiteral("nanite-clip-platform-service"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Nanite Clip"));
    QGuiApplication::setDesktopFileName(QStringLiteral("nanite-clip"));
    QGuiApplication::setOrganizationDomain(QStringLiteral("angz.dev"));
    QGuiApplication::setQuitOnLastWindowClosed(false);

    KAboutData aboutData(
        QStringLiteral("nanite-clip"),
        QStringLiteral("Nanite Clip"),
        QStringLiteral("0.1.0"),
        QStringLiteral("Nanite Clip platform service"),
        KAboutLicense::Apache_V2,
        QStringLiteral("Copyright 2026 AnotherGenZ"),
        QString(),
        QStringLiteral("https://github.com/AnotherGenZ/nanite-clip"));
    aboutData.setOrganizationDomain(QByteArrayLiteral("angz.dev"));
    aboutData.setDesktopFileName(QStringLiteral("nanite-clip"));
    KAboutData::setApplicationData(aboutData);
}

bool parseHotkeyRequest(const QJsonObject &root, HotkeyRequest *request, QString *error)
{
    const auto protocolVersion = root.value(QStringLiteral("protocol_version")).toInt(-1);
    if (protocolVersion != kProtocolVersion) {
        *error = QStringLiteral("unsupported protocol version `%1`").arg(protocolVersion);
        return false;
    }

    const auto service = root.value(QStringLiteral("service")).toString().trimmed();
    if (service != kPlasmaManualClipHotkeyService) {
        *error =
            QStringLiteral("unsupported platform service request `%1`").arg(service);
        return false;
    }

    request->combinedKey = root.value(QStringLiteral("combined_key")).toInt();
    request->actionId = root.value(QStringLiteral("action_id")).toString().trimmed();
    request->description = root.value(QStringLiteral("description")).toString().trimmed();

    if (request->combinedKey == 0) {
        *error = QStringLiteral("combined_key must be a non-zero integer");
        return false;
    }
    if (request->actionId.isEmpty() || request->description.isEmpty()) {
        *error = QStringLiteral("action_id and description are required");
        return false;
    }

    return true;
}

bool readHotkeyRequest(HotkeyRequest *request, QString *error)
{
    QTextStream input(stdin);
    input.setEncoding(QStringConverter::Utf8);

    const auto line = input.readLine();
    if (line.isNull() || line.trimmed().isEmpty()) {
        *error = QStringLiteral("missing platform service request");
        return false;
    }

    QJsonParseError parseError;
    const auto document = QJsonDocument::fromJson(line.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        *error = QStringLiteral("invalid platform service request: %1").arg(parseError.errorString());
        return false;
    }

    return parseHotkeyRequest(document.object(), request, error);
}

} // namespace

int main(int argc, char *argv[])
{
    logPhase(QStringLiteral("starting platform service"));
    if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORM")) {
        qputenv("QT_QPA_PLATFORM", QByteArrayLiteral("offscreen"));
    }

    QGuiApplication app(argc, argv);
    configureApplicationMetadata();
    logPhase(QStringLiteral("QGuiApplication initialized"));

    HotkeyRequest request;
    QString requestError;
    if (!readHotkeyRequest(&request, &requestError)) {
        writeError(requestError);
        return 2;
    }
    logPhase(QStringLiteral("request parsed for `%1`").arg(request.actionId));

    auto sessionBus = QDBusConnection::sessionBus();
    logPhase(QStringLiteral("Qt session bus connected: %1").arg(sessionBus.isConnected() ? QStringLiteral("yes") : QStringLiteral("no")));
    if (!sessionBus.isConnected()) {
        logPhase(QStringLiteral("Qt session bus last error: %1").arg(sessionBus.lastError().message()));
    }

    KDBusService dbusService(KDBusService::Multiple | KDBusService::NoExitOnFailure);
    if (!dbusService.isRegistered()) {
        const auto error = dbusService.errorMessage().trimmed();
        writeError(error.isEmpty()
                       ? QStringLiteral("KDBusService could not register the platform service")
                       : error);
        return 1;
    }
    logPhase(QStringLiteral("KDBusService registered as %1").arg(dbusService.serviceName()));

    QAction action(&app);
    action.setObjectName(request.actionId);
    action.setText(request.description);
    action.setProperty("componentName", QStringLiteral("nanite-clip"));
    action.setProperty("componentDisplayName", QStringLiteral("Nanite Clip"));
    QObject::connect(&action, &QAction::triggered, &app, []() {
        writeMessage(QJsonObject{{QStringLiteral("event"), QStringLiteral("activated")}});
    });

    const QList<QKeySequence> requestedSequences{QKeySequence(request.combinedKey)};
    auto *globalAccel = KGlobalAccel::self();
    globalAccel->setDefaultShortcut(&action, requestedSequences, KGlobalAccel::NoAutoloading);
    const bool assigned =
        globalAccel->setShortcut(&action, requestedSequences, KGlobalAccel::NoAutoloading);
    const QList<QKeySequence> activeSequences = globalAccel->shortcut(&action);

    if (!assigned || activeSequences.isEmpty()) {
        writeError(QStringLiteral("KGlobalAccel did not assign the requested shortcut"));
        return 1;
    }

    if (activeSequences.first()[0] != requestedSequences.first()[0]) {
        writeError(
            QStringLiteral("KGlobalAccel assigned `%1` instead of the requested shortcut")
                .arg(firstSequenceLabel(activeSequences)));
        return 1;
    }

    writeMessage(QJsonObject{
        {QStringLiteral("event"), QStringLiteral("ready")},
        {QStringLiteral("binding_label"), firstSequenceLabel(activeSequences)},
    });
    return app.exec();
}
