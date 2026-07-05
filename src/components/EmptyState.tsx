import type { ReactNode } from "react";
import { Icon, type IconName } from "@/components/Icon";

interface Props {
  icon: IconName;
  title: string;
  children?: ReactNode;
}

export function EmptyState({ icon, title, children }: Props) {
  return (
    <div className="empty fade-up">
      <div className="empty-icon">
        <Icon name={icon} size={26} />
      </div>
      <h3>{title}</h3>
      {children ? <p>{children}</p> : null}
    </div>
  );
}
