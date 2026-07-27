// assistant 输出多为 markdown, 但也常是纯文本 / 日志 / JSON。
// 命中任一常见 markdown 构造才走渲染, 否则保持纯文本 (UI 上仍可手动切换)。
const MARKDOWN_PATTERNS: RegExp[] = [
  /^```/m, // 围栏代码块
  /^\s{0,3}#{1,6}\s+\S/m, // 标题
  /^\s{0,3}([-*+]|\d{1,9}[.)])\s+\S/m, // 列表
  /^\s{0,3}>\s+\S/m, // 引用
  /^\s{0,3}\|.*\|\s*$/m, // 表格行
  /^\s{0,3}(-{3,}|\*{3,}|_{3,})\s*$/m, // 分隔线
  /\*\*[^\s*][^*]*\*\*/, // 加粗
  /`[^`\n]+`/, // 行内代码
  /\[[^\]\n]+\]\([^)\s]+\)/, // 链接
];

export function looksLikeMarkdown(text: string): boolean {
  return MARKDOWN_PATTERNS.some((re) => re.test(text));
}
