import 'package:glimpse/protocol/match.dart';

class Matches {
  final List<Match> items;
  Matches(this.items);

  factory Matches.fromJson(Map<String, dynamic> json) {
    return Matches((json['items'] as List<dynamic>).map((e) => Match.fromJson(e as Map<String, dynamic>)).toList());
  }
}

class RPCResponse {
  final int id;
  final dynamic result;
  final String? source;
  final String? error;

  RPCResponse(this.id, this.result, {this.source, this.error});

  factory RPCResponse.fromJson(Map<String, dynamic> json) {
    final result = switch (json['result']['type']) {
      'matches' => (json['result']['items'] as List<dynamic>).map((e) => Match.fromJson(e)).toList(),
      _ => throw UnimplementedError('Unknown MethodResult type: ${json['result']['type']}'),
    };

    return RPCResponse(json['id'] as int, result, source: json['plugin_id'] as String?, error: json['error'] as String?);
  }
}
