import 'dart:async';

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';
import 'dart:io';
import 'dart:convert';

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

  const daemonBinary = String.fromEnvironment('GLIMPSED_BIN', defaultValue: '/usr/bin/glimpsed');

  if (!File(daemonBinary).existsSync()) {
    print('Error: Daemon binary not found at $daemonBinary');
    exit(1);
  }

  runApp(const MainApp(daemonBinary: daemonBinary));
}

class _AppState extends State<MainApp> {
  late Process _process;
  final _inputController = TextEditingController();
  final _inputStreamController = StreamController<String>();

  late StreamSubscription<String> _stdinSubscription;
  late StreamSubscription<String> _stdoutSubscription;
  late StreamSubscription<String> _stderrSubscription;

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
      environment: {'GLIMPSED_PLUGIN_DIR': String.fromEnvironment('GLIMPSED_PLUGIN_DIR', defaultValue: '')},
    );

    _stdinSubscription = _inputStreamController.stream.listen((msg) {
      _process.stdin.writeln(msg);
      _process.stdin.flush();
    });

    _stdoutSubscription = _process.stdout.transform(const Utf8Decoder()).transform(const LineSplitter()).listen((data) {
      print('Daemon output: $data');
    });

    _stderrSubscription = _process.stderr.transform(const Utf8Decoder()).transform(const LineSplitter()).listen((data) {
      print('Daemon error: $data');
    });
  }

  @override
  void dispose() {
    _inputController.dispose();
    _stdinSubscription.cancel();
    _stdoutSubscription.cancel();
    _stderrSubscription.cancel();
    _inputStreamController.close();
    _process.kill();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Column(
          children: [
            Padding(
              padding: const EdgeInsets.all(8.0),
              child: Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _inputController,
                      decoration: const InputDecoration(hintText: 'Enter command'),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.send),
                    onPressed: () {
                      final input = _inputController.text;
                      if (input.isNotEmpty) {
                        _inputStreamController.add(input);
                        _inputController.clear();
                      }
                    },
                  ),
                ],
              ),
            ),
            Expanded(
              child: Container(
                color: Colors.black12,
                child: const Center(child: Text('Daemon Output Here')),
              ),
            ),
          ],
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
