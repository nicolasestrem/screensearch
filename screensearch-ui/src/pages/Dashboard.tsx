import { Network, BarChart3 } from 'lucide-react';
import { DailyDigestCard } from '../components/dashboard/DailyDigestCard';
import { MemoryStatusGauge } from '../components/dashboard/MemoryStatusGauge';
import { ProductivityPulse } from '../components/dashboard/ProductivityPulse';
import { ComingSoonCard } from '../components/ui/ComingSoonCard';

export function DashboardPage() {
  return (
    <div className="max-w-7xl mx-auto space-y-6 animate-fade-in-up">
      {/* Page Header */}
      <div className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tight gradient-text">
          Intel Dash
        </h1>
        <p className="text-muted-foreground">
          Your AI-powered productivity insights at a glance.
        </p>
      </div>

      {/* Top Row: Daily Digest + Memory Status */}
      <div className="grid gap-6 lg:grid-cols-[1fr_320px]">
        <DailyDigestCard />
        <MemoryStatusGauge />
      </div>

      {/* Middle Row: Productivity Pulse */}
      <ProductivityPulse />

      {/* Bottom Row: Coming Soon Features */}
      <div className="grid gap-6 md:grid-cols-2">
        <ComingSoonCard
          title="Knowledge Graph"
          description="Visualize connections between your activities, topics, and projects. Discover patterns in how you work."
          icon={<Network className="h-5 w-5" />}
        />
        <ComingSoonCard
          title="Analytics"
          description="Deep dive into your productivity metrics. Track focus time, app usage patterns, and meeting distribution."
          icon={<BarChart3 className="h-5 w-5" />}
        />
      </div>
    </div>
  );
}
