import 'package:dbus/dbus.dart';
import 'package:window_manager/window_manager.dart';

class GlimpseObject extends DBusObject {
  final WindowManager windowManager;
  GlimpseObject(this.windowManager) : super(DBusObjectPath('/me/aresa/Glimpse'));

  @override
  List<DBusIntrospectInterface> introspect() {
    return [
      DBusIntrospectInterface(
        'me.aresa.Glimpse',
        methods: [DBusIntrospectMethod('toggle'), DBusIntrospectMethod('activate'), DBusIntrospectMethod('ping')],
        signals: [],
        properties: [],
      ),
    ];
  }

  @override
  Future<DBusMethodResponse> handleMethodCall(DBusMethodCall methodCall) async {
    if (methodCall.interface != 'me.aresa.Glimpse') {
      return DBusMethodErrorResponse.unknownInterface();
    }

    switch (methodCall.name) {
      case 'ping':
        return DBusMethodSuccessResponse([DBusString('pong')]);
      case 'activate':
        await windowManager.show();
        await windowManager.focus();
        return DBusMethodSuccessResponse([DBusString('activated')]);
      case 'toggle':
        if (await windowManager.isVisible()) {
          await windowManager.hide();
        } else {
          await windowManager.show();
          await windowManager.focus();
        }
        return DBusMethodSuccessResponse([DBusString('toggled')]);
      default:
        return DBusMethodErrorResponse.unknownMethod();
    }
  }
}

Future<bool> isDBusServiceRunning() async {
  var client = DBusClient.session();
  try {
    var proxy = DBusRemoteObject(client, name: 'me.aresa.Glimpse', path: DBusObjectPath('/me/aresa/Glimpse'));
    var result = await proxy.callMethod('me.aresa.Glimpse', 'ping', [], replySignature: DBusSignature('s'));
    if (result.returnValues[0].asString() != 'pong') {
      throw 'Unexpected response from D-Bus service';
    }
    return true;
  } catch (e) {
    return false;
  }
}

Future<void> activateRunningInstance() async {
  var client = DBusClient.session();
  var proxy = DBusRemoteObject(client, name: 'me.aresa.Glimpse', path: DBusObjectPath('/me/aresa/Glimpse'));
  await proxy.callMethod('me.aresa.Glimpse', 'toggle', [], replySignature: DBusSignature('s'));
}

Future<void> initializeDBusService(WindowManager windowManager) async {
  var client = DBusClient.session();
  await client.requestName('me.aresa.Glimpse');
  await client.registerObject(GlimpseObject(windowManager));
}
