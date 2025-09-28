import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:glimpse/protocol/request.dart';
import 'package:glimpse/protocol/response.dart';
import 'package:glimpse/protocol/match.dart';
import 'package:window_manager/window_manager.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await windowManager.ensureInitialized();
  WindowOptions windowOptions = WindowOptions(
    size: Size(700, 500),
    center: true,
    backgroundColor: Colors.transparent,
    skipTaskbar: true,
    alwaysOnTop: true,
    windowButtonVisibility: false,
    titleBarStyle: TitleBarStyle.hidden,
  );

  windowManager.waitUntilReadyToShow(windowOptions, () async {
    await windowManager.show();
    await windowManager.focus();
  });
  final daemonBinary = Platform.environment['GLIMPSED_BIN'] ?? '/usr/bin/glimpsed';
  print('Using daemon binary at $daemonBinary');
  print('Using plugin directory at ${Platform.environment['GLIMPSE_PLUGIN_DIR'] ?? ''}');

  if (!File(daemonBinary).existsSync()) {
    print('Error: Daemon binary not found at $daemonBinary');
    exit(1);
  }

  runApp(MainApp(daemonBinary: daemonBinary));
}

class _AppState extends State<MainApp> {
  late Process _process;
  int id = 1;
  final _inputController = TextEditingController();
  final _inputStreamController = StreamController<Method>();
  final _searchItems = <Match>[];

  late StreamSubscription<Method> _stdinSubscription;
  late StreamSubscription<String> _stdoutSubscription;
  late StreamSubscription<String> _stderrSubscription;
  final _popupMenuKey = GlobalKey<PopupMenuButtonState<int>>();
  final _inputFocusNode = FocusNode();
  int selectedIndex = -1;
  Timer? _debounceTimer;

  @override
  void initState() {
    super.initState();
    _startDaemon();
  }

  Future<void> _startDaemon() async {
    _process = await Process.start(
      widget.daemonBinary,
      [],
      mode: ProcessStartMode.normal,
      includeParentEnvironment: true,
    );

    _stdinSubscription = _inputStreamController.stream.listen((method) async {
      id += 1;
      final request = RPCRequest(id, method);
      _process.stdin.writeln(request.toJsonString());
      await _process.stdin.flush();
    });

    _stdoutSubscription = _process.stdout.transform(const Utf8Decoder()).transform(const LineSplitter()).listen((data) {
      final message = RPCResponse.fromJson(jsonDecode(data));
      switch (message.result) {
        case List<Match> items:
          addSearchItems(items);
          break;
        default:
          break;
      }
    });

    _stderrSubscription = _process.stderr.transform(const Utf8Decoder()).transform(const LineSplitter()).listen((data) {
      print(data);
    });
  }

  @override
  void dispose() {
    _inputController.dispose();
    _stdinSubscription.cancel();
    _stdoutSubscription.cancel();
    _stderrSubscription.cancel();
    _inputStreamController.close();
    _inputFocusNode.dispose();
    _process.kill();
    super.dispose();
  }

  void addSearchItems(List<Match> items) {
    setState(() {
      _searchItems.clear();
      _searchItems.addAll(items);
    });
    if (items.isNotEmpty) {
      selectedIndex = 0;
    } else {
      selectedIndex = -1;
    }
  }

  KeyEventResult selectNextItem(int direction) {
    setState(() {
      selectedIndex += direction;
      if (selectedIndex < 0) {
        selectedIndex = 0;
      } else if (selectedIndex >= _searchItems.length) {
        selectedIndex = _searchItems.length - 1;
      }
    });

    return KeyEventResult.handled;
  }

  KeyEventResult activateAction(int itemIndex, {int actionIndex = 0}) {
    print('Activating default action for selected index: $actionIndex');
    if (itemIndex < 0 || itemIndex >= _searchItems.length) {
      print('No item selected or index out of range');
      return KeyEventResult.handled;
    }

    final item = _searchItems[itemIndex];
    if (item.actions.isEmpty) {
      print('No actions available for the selected item');
      return KeyEventResult.handled;
    }

    var action = item.actions.first;
    if (actionIndex > 0 && item.actions.length > actionIndex) {
      action = item.actions[actionIndex];
    }

    print('Activating action: ${action} for item: ${item.title}');

    _inputStreamController.add(Activate(itemIndex, actionIndex));
    if (action.closeOnAction) {
      // windowManager.close();
    }
    return KeyEventResult.handled;
  }

  KeyEventResult showActionMenu(int itemIndex) {
    if (itemIndex < 0 || itemIndex >= _searchItems.length) {
      print('Invalid item index');
      return KeyEventResult.handled;
    }

    final item = _searchItems[itemIndex];
    if (item.actions.isEmpty) {
      print('No actions available for the selected item');
      return KeyEventResult.handled;
    }

    // Show the action menu (implementation depends on your UI framework)
    print('Showing action menu for item: ${item.title}');
    _popupMenuKey.currentState?.showButtonMenu();
    return KeyEventResult.handled;
  }

  void onSearchInputChanged(String value) {
    if (value.isNotEmpty) {
      _inputStreamController.add(SearchMethod(value));
      FocusScope.of(context).requestFocus(_inputFocusNode);
    }
  }

  KeyEventResult handleEsc() {
    if (_inputController.text.isNotEmpty) {
      setState(() {
        _inputController.clear();
        _searchItems.clear();
        selectedIndex = -1;
      });
      FocusScope.of(context).requestFocus(_inputFocusNode);
    } else {
      windowManager.close();
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.light,
        primarySwatch: Colors.grey,
        scaffoldBackgroundColor: Colors.grey[100],
        useSystemColors: true,
        inputDecorationTheme: InputDecorationTheme(
          filled: false,
          fillColor: Colors.grey[800],
          border: OutlineInputBorder(borderSide: BorderSide.none),
          hintStyle: TextStyle(color: Colors.grey[900]),
        ),
      ),
      home: Focus(
        onKeyEvent: (node, event) => switch (event is KeyDownEvent ? event.logicalKey : null) {
          LogicalKeyboardKey.arrowDown => selectNextItem(1),
          LogicalKeyboardKey.arrowUp => selectNextItem(-1),
          LogicalKeyboardKey.escape => handleEsc(),
          LogicalKeyboardKey.keyK => switch (HardwareKeyboard.instance.isAltPressed) {
            true => showActionMenu(selectedIndex),
            false => KeyEventResult.ignored,
          },
          LogicalKeyboardKey.enter =>
            HardwareKeyboard.instance.isShiftPressed
                ? activateAction(selectedIndex, actionIndex: 1)
                : HardwareKeyboard.instance.isAltPressed
                ? showActionMenu(selectedIndex)
                : activateAction(selectedIndex),
          _ => KeyEventResult.ignored,
        },
        child: Scaffold(
          body: Column(
            children: [
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _inputController,
                      decoration: const InputDecoration(hintText: 'Start typing to search...'),
                      autofocus: true,
                      canRequestFocus: true,
                      focusNode: _inputFocusNode,
                      onChanged: (value) {
                        _debounceTimer?.cancel();
                        _debounceTimer = Timer(const Duration(milliseconds: 50), () {
                          if (_inputController.text == value) {
                            onSearchInputChanged(value);
                          }
                        });
                      },
                      onSubmitted: onSearchInputChanged,
                    ),
                  ),
                ],
              ),
              Expanded(
                child: ListView.builder(
                  itemCount: _searchItems.length,
                  itemBuilder: (context, index) {
                    final item = _searchItems[index];
                    final isSelected = index == selectedIndex;
                    if (isSelected) {
                      WidgetsBinding.instance.addPostFrameCallback((_) {
                        Scrollable.ensureVisible(context, duration: const Duration(milliseconds: 100), alignment: 0.5);
                      });
                    }
                    return PopupMenuButton<int>(
                      key: selectedIndex == index ? _popupMenuKey : null,
                      enabled: selectedIndex == index && item.actions.isNotEmpty,
                      onSelected: (value) => activateAction(selectedIndex, actionIndex: value),
                      itemBuilder: (BuildContext context) => item.actions.asMap().entries.map((entry) {
                        final actionIndex = entry.key;
                        final action = entry.value;
                        return PopupMenuItem<int>(value: actionIndex, child: Text(action.title));
                      }).toList(),
                      child: ListTile(
                        title: Text(item.title),
                        subtitle: Text(item.description),
                        selected: isSelected,
                        focusColor: isSelected ? Colors.blue : null,
                        hoverColor: Colors.grey[300],
                        tileColor: isSelected ? Colors.blue[500] : null,
                        onTap: () => activateAction(index),
                        selectedColor: Colors.black,
                        selectedTileColor: Colors.grey[300],
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class MainApp extends StatefulWidget {
  final String daemonBinary;
  const MainApp({super.key, required this.daemonBinary});

  @override
  State<MainApp> createState() => _AppState();
}
