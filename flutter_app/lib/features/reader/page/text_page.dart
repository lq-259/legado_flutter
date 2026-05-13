class TextPage {
  final int chapterIndex;
  final int pageIndex;
  final int startParagraphIndex;
  final int endParagraphIndex;
  final int startCharOffset;
  final int endCharOffset;
  final List<String> paragraphTexts;
  final String? headerText;
  final double contentHeight;
  final bool isCompleted;

  const TextPage({
    required this.chapterIndex,
    required this.pageIndex,
    required this.startParagraphIndex,
    required this.endParagraphIndex,
    required this.startCharOffset,
    required this.endCharOffset,
    required this.paragraphTexts,
    this.headerText,
    this.contentHeight = 0,
    this.isCompleted = true,
  });

  int get totalChars =>
      paragraphTexts.fold(0, (sum, p) => sum + p.length);

  @override
  String toString() =>
      'TextPage(ch=$chapterIndex, pg=$pageIndex, paras=[$startParagraphIndex-$endParagraphIndex], chars=$totalChars)';
}
