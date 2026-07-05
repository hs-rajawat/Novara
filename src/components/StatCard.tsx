import { Icon, type IconName } from "@/components/Icon";

export type StatTone = "violet" | "cyan" | "green" | "gold";

interface Props {
  icon: IconName;
  label: string;
  value: string;
  tone?: StatTone;
  /** Stagger index for the entrance animation. */
  index?: number;
}

export function StatCard({ icon, label, value, tone = "violet", index = 0 }: Props) {
  return (
    <div className="stat-card fade-up" style={{ animationDelay: `${index * 60}ms` }}>
      <div className={`stat-icon tone-${tone}`}>
        <Icon name={icon} size={20} />
      </div>
      <div className="stat-body">
        <div className="stat-label">{label}</div>
        <div className="stat-value">{value}</div>
      </div>
    </div>
  );
}
