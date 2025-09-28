import 'dart:convert';

abstract class Method {
  String get methodName;
  dynamic asParams();
}

class SearchMethod extends Method {
  final String query;

  @override
  String get methodName => 'search';

  @override
  dynamic asParams() => query;

  SearchMethod(this.query);
}

class Activate extends Method {
  final int itemIndex;
  final int actionIndex;

  @override
  String get methodName => 'activate';

  @override
  dynamic asParams() => [itemIndex, actionIndex];

  Activate(this.itemIndex, this.actionIndex);
}

class RPCRequest {
  final int id;
  final Method method;
  final String? target;
  final String? context;

  RPCRequest(this.id, this.method, {this.target, this.context});

  Map<String, dynamic> toJson() {
    return {'id': id, 'method': method.methodName, 'params': method.asParams(), 'target': target, 'context': context};
  }

  String toJsonString() {
    return jsonEncode(toJson());
  }
}
