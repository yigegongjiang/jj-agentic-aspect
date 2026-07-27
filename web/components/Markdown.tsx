'use client';

import { memo } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkBreaks from 'remark-breaks';
import remarkGfm from 'remark-gfm';

interface MdNode {
  type: string;
  children?: MdNode[];
}

// 不接 rehype-raw (会开 XSS 面), 而 remark-rehype 默认把裸 HTML 节点直接丢弃。
// 降级成纯文本, 保证 assistant 写的 <Foo /> 之类不会静默消失。
function remarkHtmlAsText() {
  return (tree: MdNode) => {
    const walk = (node: MdNode) => {
      if (node.type === 'html') node.type = 'text';
      node.children?.forEach(walk);
    };
    walk(tree);
  };
}

// 文字颜色一律继承外层 (assistant 亮块 / progress 淡块共用同一个组件),
// 只对链接 / 代码 / 引用 / 表格做最小强调。字号沿用容器 text-sm。
const COMPONENTS: Components = {
  p: ({ children }) => <p className="my-1.5 first:mt-0 last:mb-0">{children}</p>,
  h1: ({ children }) => (
    <h1 className="mt-3 mb-1 text-[15px] font-semibold first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mt-3 mb-1 text-[14px] font-semibold first:mt-0">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mt-2.5 mb-1 text-[13px] font-semibold first:mt-0">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="mt-2 mb-0.5 text-[13px] font-semibold opacity-90 first:mt-0">{children}</h4>
  ),
  h5: ({ children }) => (
    <h5 className="mt-2 mb-0.5 text-xs font-semibold opacity-90 first:mt-0">{children}</h5>
  ),
  h6: ({ children }) => (
    <h6 className="mt-2 mb-0.5 text-xs font-semibold opacity-80 first:mt-0">{children}</h6>
  ),
  ul: ({ children }) => (
    <ul className="my-1.5 pl-4 list-disc marker:text-zinc-600 space-y-0.5 first:mt-0 last:mb-0 [&_ul]:my-0.5 [&_ol]:my-0.5">
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className="my-1.5 pl-5 list-decimal marker:text-zinc-500 space-y-0.5 first:mt-0 last:mb-0 [&_ul]:my-0.5 [&_ol]:my-0.5">
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="[&>p]:my-0">{children}</li>,
  blockquote: ({ children }) => (
    <blockquote className="my-1.5 pl-3 border-l-2 border-zinc-700 opacity-80">{children}</blockquote>
  ),
  code: ({ children }) => (
    <code className="rounded bg-zinc-800/70 px-1 py-px font-mono text-[0.85em] text-zinc-200">
      {children}
    </code>
  ),
  // 围栏代码块: 复用 code 的样式重置, 自身负责滚动与边框
  pre: ({ children }) => (
    <pre className="my-2 p-2.5 rounded-md border border-zinc-800 bg-zinc-950/80 overflow-x-auto text-xs leading-relaxed text-zinc-300 [&>code]:bg-transparent [&>code]:p-0 [&>code]:text-inherit [&>code]:text-[length:inherit]">
      {children}
    </pre>
  ),
  a: ({ href, children }) => (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className="text-blue-300 underline underline-offset-2 hover:text-blue-200 break-all"
    >
      {children}
    </a>
  ),
  strong: ({ children }) => <strong className="font-semibold text-zinc-50">{children}</strong>,
  em: ({ children }) => <em className="italic">{children}</em>,
  del: ({ children }) => <del className="opacity-60 line-through">{children}</del>,
  hr: () => <hr className="my-2 border-zinc-800" />,
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className="text-xs border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-zinc-800 bg-zinc-900/60 px-2 py-1 text-left font-semibold whitespace-nowrap">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-zinc-800 px-2 py-1 align-top">{children}</td>
  ),
  // GFM task list: 只读复选框
  input: ({ checked }) => (
    <input
      type="checkbox"
      checked={checked ?? false}
      readOnly
      className="mr-1 align-[-1px] accent-zinc-500"
    />
  ),
  img: ({ src, alt }) => (
    <img src={typeof src === 'string' ? src : ''} alt={alt ?? ''} className="my-1.5 max-w-full rounded" />
  ),
};

// remark-breaks: 单换行也断行 —— agent 输出常靠单换行分行, 不加会被合并成一段
const PLUGINS = [remarkGfm, remarkBreaks, remarkHtmlAsText];

// SessionDetail 每 5s 轮询重渲染, memo 避免重复解析 markdown
export default memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="text-sm leading-relaxed break-words">
      <ReactMarkdown remarkPlugins={PLUGINS} components={COMPONENTS}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
