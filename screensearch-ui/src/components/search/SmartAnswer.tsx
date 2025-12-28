import { motion } from 'framer-motion';
import { Sparkles, Chrome, Code2, MessageSquare, FileText, Terminal, Globe } from 'lucide-react';
import ReactMarkdown from 'react-markdown';

export interface ActivitySource {
  id: number;
  app: string;
  context: string;
  timeAgo: string;
}

interface SmartAnswerProps {
  answer: string;
  sources: ActivitySource[];
}

// App icon mapping
const appIcons: Record<string, React.ElementType> = {
  'chrome': Chrome,
  'google chrome': Chrome,
  'firefox': Globe,
  'safari': Globe,
  'edge': Globe,
  'vs code': Code2,
  'visual studio code': Code2,
  'code': Code2,
  'slack': MessageSquare,
  'discord': MessageSquare,
  'teams': MessageSquare,
  'terminal': Terminal,
  'iterm': Terminal,
  'warp': Terminal,
  'notion': FileText,
  'obsidian': FileText,
  'notes': FileText,
};

function getAppIcon(appName: string): React.ElementType {
  const normalizedName = appName.toLowerCase();
  for (const [key, Icon] of Object.entries(appIcons)) {
    if (normalizedName.includes(key)) {
      return Icon;
    }
  }
  return Globe; // Default icon
}

export function SmartAnswer({ answer, sources }: SmartAnswerProps) {
  return (
    <div className="space-y-4">
      {/* AI Answer Section */}
      <div className="flex items-start gap-4">
        <div className="p-2.5 rounded-xl bg-primary/20 glow-cyan-subtle flex-shrink-0">
          <Sparkles className="w-5 h-5 text-primary" />
        </div>
        <div className="flex-1 min-w-0">
          <span className="text-xs text-primary font-medium uppercase tracking-wider">
            Smart Answer
          </span>
          <div className="mt-2 prose prose-sm dark:prose-invert max-w-none">
            <ReactMarkdown
              components={{
                p: ({ children }) => (
                  <p className="text-foreground leading-relaxed">{children}</p>
                ),
                ul: ({ children }) => (
                  <ul className="space-y-1 list-none pl-0 mt-2">{children}</ul>
                ),
                li: ({ children }) => (
                  <li className="flex items-start gap-2 text-foreground">
                    <span className="w-1.5 h-1.5 rounded-full bg-primary mt-2 flex-shrink-0" />
                    <span>{children}</span>
                  </li>
                ),
              }}
            >
              {answer}
            </ReactMarkdown>
          </div>
        </div>
      </div>

      {/* Activity Sources Timeline */}
      {sources.length > 0 && (
        <div className="pt-4 border-t border-glass-border/30">
          <span className="text-xs text-muted-foreground uppercase tracking-wider mb-3 block">
            Based on activity from
          </span>
          <div className="space-y-1">
            {sources.map((source, index) => {
              const AppIcon = getAppIcon(source.app);
              return (
                <motion.div
                  key={source.id}
                  initial={{ opacity: 0, x: -20 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ delay: index * 0.1 }}
                  className="flex items-center gap-3 p-3 rounded-lg bg-surface-1/50 hover:bg-surface-2/50 border border-transparent hover:border-primary/20 transition-all cursor-pointer group"
                >
                  <div className="p-1.5 rounded-lg bg-surface-2 group-hover:bg-primary/20 transition-colors">
                    <AppIcon className="w-4 h-4 text-muted-foreground group-hover:text-primary transition-colors" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <span className="font-medium text-foreground text-sm">{source.app}</span>
                    <span className="text-muted-foreground text-sm mx-2">-</span>
                    <span className="text-muted-foreground text-sm truncate">{source.context}</span>
                  </div>
                  <span className="text-xs text-muted-foreground flex-shrink-0">
                    {source.timeAgo}
                  </span>
                </motion.div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
