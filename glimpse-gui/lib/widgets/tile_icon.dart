import 'dart:io';

import 'package:flutter/material.dart';

class TileIcon extends StatelessWidget {
  final String path;
  final double size;
  const TileIcon({super.key, required this.path, this.size = 40});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      margin: const EdgeInsets.only(right: 10),
      child: path.startsWith('http://') || path.startsWith('https://')
          ? Image.network(
              path,
              width: size,
              height: size,
              errorBuilder: (context, error, stackTrace) {
                return Icon(Icons.image_not_supported, size: size);
              },
            )
          : Image.file(
              File(path.substring(6)),
              width: size,
              height: size,
              errorBuilder: (context, error, stackTrace) {
                return Icon(Icons.image_not_supported, size: size);
              },
            ),
    );
  }
}
