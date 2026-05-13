# 阅读功能完善计划

## 目标
把当前简单阅读页升级为接近原 Legado 的沉浸式阅读器，支持连续滚动、点击控件、界面设置、背景/字体/排版配置，并持久化设置。

## 一、阅读器交互结构
1. 默认进入阅读页后只显示：
   - 系统状态栏
   - 正文内容
   - 阅读背景
2. 不常驻显示：AppBar、底部栏、目录/刷新/下载按钮
3. 点击屏幕中间区域：显示/隐藏阅读控件
4. 点击屏幕左/右区域：
   - 仅在"点击翻章"模式下触发上一章/下一章
   - 在"连续滚动"模式下不翻章
5. 控件显示时：
   - 顶部半透明栏：返回、目录、刷新、下载、关闭
   - 底部半透明栏：上一章、章节进度、下一章、界面

## 二、阅读设置模型 (ReaderSettings)
新增 `ReaderSettings`，持久化到现有 `settings.json`。

字段：
- `fontSize` (double): 字号
- `fontWeight` (int): 字重 (400/700/900)
- `fontFamily` (String?): 字体
- `textColor` (int): 文字颜色
- `backgroundColor` (int): 背景颜色
- `backgroundImagePath` (String?): 背景图片路径
- `letterSpacing` (double): 字距
- `lineHeight` (double): 行距倍数
- `paragraphSpacing` (double): 段距
- `horizontalPadding` (double): 左右边距
- `verticalPadding` (double): 上下边距
- `paragraphIndent` (String): 段首缩进
- `pageMode` (ReaderPageMode): 翻页方式

## 三、翻页模式
```dart
enum ReaderPageMode {
  continuousScroll, // 连续滚动，章节收尾相连
  tapChapter,       // 点击左右区域翻章
  page,             // 分页（后续支持）
}
```

第一版实现 continuousScroll 和 tapChapter。page 先保留选项，暂时映射到 tapChapter。

## 四、连续滚动模式
- 章节内容连续拼接
- 当前章读完后下一章直接接在后面
- 滚动距离底部 < 800px 时自动追加下一章
- 不再需要末尾上滑触发下一章
- 预加载下一章，追加后继续预加载后续章节

## 五、点击翻章模式
- 保留现有逻辑，但沉浸化
- 左侧点击：上一章
- 右侧点击：下一章
- 中间点击：显示/隐藏控件

## 六、界面设置面板
showModalBottomSheet 实现底部面板。

面板结构：
1. 字体：字号 slider、字重选择、字体选择
2. 排版：字距、行距、段距、左右边距、上下边距、缩进
3. 背景：预设颜色 + 自定义背景图
4. 翻页方式：连续滚动 / 点击翻章 / 分页（暂不支持）

## 七、正文渲染
- 段落渲染，每段加 paragraphIndent 和 paragraphSpacing
- 使用 TextStyle 设置 fontSize、fontWeight、fontFamily、letterSpacing、height
- 连续滚动用 ListView.builder

## 八、背景渲染
- 背景色：Container(color: ...)
- 背景图片：DecorationImage，选图片时覆盖在底色上

## 九、实施顺序
1. 新增 ReaderSettings 和持久化函数
2. 改造 ReaderPage 状态（controlsVisible、loadedChapters、isAppendingChapter）
3. 抽出章节内容加载函数
4. 实现连续滚动章节追加
5. 改造阅读页为 Stack 沉浸式布局
6. 添加顶部/底部控件
7. 添加界面 BottomSheet
8. 应用字体、背景、排版设置
9. 点击翻章模式兼容
10. 编译 APK 并安装测试
