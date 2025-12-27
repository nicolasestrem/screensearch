import { formatDistanceToNow } from 'date-fns';
import {
  Code2,
  Globe,
  MessageSquare,
  FileText,
  Terminal,
  Mail,
  Calendar,
  FolderOpen,
  Music,
  Video,
  Image,
  Settings,
  LucideIcon,
} from 'lucide-react';
import { cn } from '../../lib/utils';

export interface ActivityItem {
  appName: string;
  description: string;
  timestamp: Date;
  duration?: string;
  count?: number;
}

interface ActivityListProps {
  items: ActivityItem[];
  className?: string;
}

// Map app names to icons and colors
const appConfig: Record<string, { icon: LucideIcon; color: string }> = {
  'VS Code': { icon: Code2, color: 'text-blue-400' },
  'Visual Studio Code': { icon: Code2, color: 'text-blue-400' },
  Code: { icon: Code2, color: 'text-blue-400' },
  Chrome: { icon: Globe, color: 'text-yellow-400' },
  Firefox: { icon: Globe, color: 'text-orange-400' },
  Safari: { icon: Globe, color: 'text-blue-500' },
  Edge: { icon: Globe, color: 'text-cyan-400' },
  Slack: { icon: MessageSquare, color: 'text-purple-400' },
  Discord: { icon: MessageSquare, color: 'text-indigo-400' },
  Teams: { icon: MessageSquare, color: 'text-blue-500' },
  Notion: { icon: FileText, color: 'text-gray-400' },
  Word: { icon: FileText, color: 'text-blue-600' },
  Terminal: { icon: Terminal, color: 'text-green-400' },
  'Windows Terminal': { icon: Terminal, color: 'text-green-400' },
  iTerm: { icon: Terminal, color: 'text-green-400' },
  Outlook: { icon: Mail, color: 'text-blue-500' },
  Gmail: { icon: Mail, color: 'text-red-400' },
  Calendar: { icon: Calendar, color: 'text-blue-400' },
  Explorer: { icon: FolderOpen, color: 'text-yellow-500' },
  Finder: { icon: FolderOpen, color: 'text-blue-400' },
  Spotify: { icon: Music, color: 'text-green-500' },
  YouTube: { icon: Video, color: 'text-red-500' },
  Figma: { icon: Image, color: 'text-purple-500' },
  Photoshop: { icon: Image, color: 'text-blue-500' },
  Settings: { icon: Settings, color: 'text-gray-400' },
};

function getAppConfig(appName: string) {
  // Try exact match first
  if (appConfig[appName]) {
    return appConfig[appName];
  }

  // Try partial match
  const lowerName = appName.toLowerCase();
  for (const [key, config] of Object.entries(appConfig)) {
    if (lowerName.includes(key.toLowerCase()) || key.toLowerCase().includes(lowerName)) {
      return config;
    }
  }

  // Default
  return { icon: FolderOpen, color: 'text-muted-foreground' };
}

export function ActivityList({ items, className }: ActivityListProps) {
  if (items.length === 0) {
    return null;
  }

  return (
    <div className={cn('space-y-3', className)}>
      {items.map((item, index) => {
        const config = getAppConfig(item.appName);
        const Icon = config.icon;

        return (
          <div
            key={index}
            className="flex items-start gap-3 p-3 rounded-lg bg-muted/30 hover:bg-muted/50 transition-colors animate-fade-in-up"
            style={{ animationDelay: `${index * 50}ms` }}
          >
            {/* App Icon */}
            <div
              className={cn(
                'flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center',
                'bg-background/50 border border-border/50'
              )}
            >
              <Icon className={cn('h-5 w-5', config.color)} />
            </div>

            {/* Content */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="font-medium text-foreground truncate">
                  {item.appName}
                </span>
                {item.duration && (
                  <span className="text-xs text-primary font-medium">
                    {item.duration}
                  </span>
                )}
              </div>
              <p className="text-sm text-muted-foreground truncate">
                {item.description}
              </p>
            </div>

            {/* Timestamp */}
            <div className="flex-shrink-0 text-right">
              <span className="text-xs text-muted-foreground">
                {formatDistanceToNow(item.timestamp, { addSuffix: true })}
              </span>
              {item.count && item.count > 1 && (
                <div className="text-xs text-primary/70 mt-0.5">
                  {item.count} captures
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
