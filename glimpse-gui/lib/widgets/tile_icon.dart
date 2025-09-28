import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_svg/svg.dart';

class TileIcon extends StatelessWidget {
  final String path;
  final double size;
  const TileIcon({super.key, required this.path, this.size = 40});

  Icon get defaultIcon => Icon(Icons.photo, size: size);
  Icon get errorIcon => Icon(Icons.broken_image, size: size);

  Widget buildIcon() {
    if (path.isEmpty) {
      return defaultIcon;
    }

    if (path.toLowerCase().endsWith('.svg')) {
      return buildSVGImage(path);
    }

    if (path.startsWith('http://') || path.startsWith('https://')) {
      return Image.network(path, width: size, height: size, errorBuilder: (context, error, stackTrace) => errorIcon);
    }

    final file = File(path);
    if (file.existsSync()) {
      return Image.file(file, width: size, height: size, errorBuilder: (context, error, stackTrace) => errorIcon);
    }
    return defaultIcon;
  }

  Widget buildSVGImage(String path) {
    if (path.startsWith('http://') || path.startsWith('https://')) {
      return SvgPicture.network(path, width: size, height: size, placeholderBuilder: (context) => defaultIcon);
    }
    return SvgPicture.file(File(path), width: size, height: size, placeholderBuilder: (context) => defaultIcon);
  }

  @override
  Widget build(BuildContext context) {
    return Container(width: size, height: size, margin: const EdgeInsets.only(right: 10), child: buildIcon());
  }
}
