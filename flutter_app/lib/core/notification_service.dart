import 'package:flutter/services.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';

class NotificationService {
  static const _channel = MethodChannel('legado/notifications');
  static final FlutterLocalNotificationsPlugin _plugin =
      FlutterLocalNotificationsPlugin();

  /// Must match MainActivity.kt DOWNLOAD_CHANNEL_ID
  static const String _downloadChannelId = 'legado_download';
  static const String _downloadChannelName = '下载通知';

  static bool _initialized = false;

  static Future<void> init() async {
    if (_initialized) return;
    _initialized = true;

    const androidSettings = AndroidInitializationSettings('ic_notification');
    const iosSettings = DarwinInitializationSettings(
      requestAlertPermission: false,
      requestBadgePermission: false,
      requestSoundPermission: false,
    );

    await _plugin.initialize(
      const InitializationSettings(
        android: androidSettings,
        iOS: iosSettings,
      ),
    );

    final androidPlugin = _plugin.resolvePlatformSpecificImplementation<
        AndroidFlutterLocalNotificationsPlugin>();
    if (androidPlugin != null) {
      final channel = AndroidNotificationChannel(
        _downloadChannelId,
        _downloadChannelName,
        description: '书籍下载进度通知',
        importance: Importance.low,
      );
      await androidPlugin.createNotificationChannel(channel);
    }
  }

  static Future<void> showDownloadProgress({
    required int id,
    required String title,
    required int current,
    required int total,
  }) async {
    final progress = total > 0 ? current * 100 ~/ total : 0;
    await _plugin.show(
      id,
      title,
      '下载中 $current/$total ($progress%)',
      NotificationDetails(
        android: AndroidNotificationDetails(
          _downloadChannelId,
          _downloadChannelName,
          channelDescription: '书籍下载进度通知',
          importance: Importance.low,
          priority: Priority.low,
          ongoing: true,
          onlyAlertOnce: true,
          showProgress: true,
          maxProgress: total,
          progress: current,
          icon: 'ic_notification',
        ),
        iOS: const DarwinNotificationDetails(
          presentAlert: false,
          presentBadge: true,
          presentSound: false,
        ),
      ),
    );
  }

  static Future<void> showDownloadComplete({
    required int id,
    required String title,
    required int successCount,
    required int failCount,
    required int skipCount,
  }) async {
    final hasFailures = failCount > 0 || skipCount > 0;
    await _plugin.show(
      id,
      title,
      hasFailures
          ? '完成 (成功: $successCount, 失败: $failCount, 跳过: $skipCount)'
          : '下载完成 ($successCount 章)',
      NotificationDetails(
        android: AndroidNotificationDetails(
          _downloadChannelId,
          _downloadChannelName,
          channelDescription: '书籍下载进度通知',
          importance: Importance.defaultImportance,
          priority: Priority.defaultPriority,
          ongoing: false,
          autoCancel: true,
          icon: 'ic_notification',
        ),
        iOS: const DarwinNotificationDetails(
          presentAlert: true,
          presentBadge: true,
          presentSound: true,
        ),
      ),
    );
  }

  static Future<void> cancelNotification(int id) async {
    await _plugin.cancel(id);
  }

  static Future<bool> hasPermission() async {
    final result = await _channel.invokeMethod<bool>('hasPermission');
    return result ?? false;
  }

  static Future<bool> requestPermission() async {
    final result = await _channel.invokeMethod<bool>('requestPermission');
    return result ?? false;
  }

  static Future<void> openNotificationSettings() async {
    await _channel.invokeMethod('openNotificationSettings');
  }
}
