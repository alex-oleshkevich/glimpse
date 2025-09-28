sealed class ActionHandler {}

class ShellExecHandler extends ActionHandler {
  final String command;
  final List<String> args;
  ShellExecHandler(this.command, this.args);

  factory ShellExecHandler.fromJson(Map<String, dynamic> json) {
    return ShellExecHandler(
      json['command'] as String,
      (json['args'] as List<dynamic>).map((e) => e as String).toList(),
    );
  }
}

class LaunchHandler extends ActionHandler {
  final String appId;
  final String? action;
  LaunchHandler(this.appId, this.action);

  factory LaunchHandler.fromJson(Map<String, dynamic> json) {
    return LaunchHandler(json['app_id'] as String, json['action'] as String?);
  }
}

class OpenURIHandler extends ActionHandler {
  final String path;
  OpenURIHandler(this.path);

  factory OpenURIHandler.fromJson(Map<String, dynamic> json) {
    return OpenURIHandler(json['uri'] as String);
  }
}

class ClipboardHandler extends ActionHandler {
  final String content;
  ClipboardHandler(this.content);

  factory ClipboardHandler.fromJson(Map<String, dynamic> json) {
    return ClipboardHandler(json['text'] as String);
  }
}

class CallbackAction extends ActionHandler {
  final String name;
  final Map<String, dynamic> parameters;
  CallbackAction(this.name, this.parameters);

  factory CallbackAction.fromJson(Map<String, dynamic> json) {
    return CallbackAction(json['key'] as String, json['params'] as Map<String, dynamic>);
  }
}

final class MatchAction {
  final String title;
  final ActionHandler action;
  final bool closeOnAction;

  MatchAction(this.title, this.action, {this.closeOnAction = true});
}

final class Match {
  final String title;
  final String description;
  final String? icon;
  final double? score;
  final List<MatchAction> actions;

  Match(this.title, this.description, {this.icon, this.score, this.actions = const []});

  factory Match.fromJson(Map<String, dynamic> json) {
    return Match(
      json['title'] as String,
      json['description'] as String,
      icon: json['icon'] as String?,
      score: (json['score'] as num?)?.toDouble(),
      actions: (json['actions'] as List<dynamic>? ?? []).map((actionItem) {
        final actionJson = actionItem['action'] as Map<String, dynamic>;
        final action = switch (actionJson['type']) {
          'exec' => ShellExecHandler.fromJson(actionJson),
          'open' => OpenURIHandler.fromJson(actionJson),
          'clipboard' => ClipboardHandler.fromJson(actionJson),
          'callback' => CallbackAction.fromJson(actionJson),
          'launch' => LaunchHandler.fromJson(actionJson),
          _ => throw Exception('Unknown action type: ${actionJson['type']}'),
        };
        return MatchAction(actionItem['title'], action, closeOnAction: actionItem['close_on_action'] ?? true);
      }).toList(),
    );
  }
}
