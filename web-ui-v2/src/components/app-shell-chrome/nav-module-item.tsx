import type { ReactElement } from "react";
import { Link } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  useSidebar,
} from "@/components/ui/sidebar";
import {
  activeNavTarget,
  isNavGroup,
  resolveNavLabel,
  type NavGroup,
  type NavLink,
  type NavModule,
  type NavNode,
} from "./nav";

function nodeIsActive(node: NavNode, activeTo: string | undefined): boolean {
  if (isNavGroup(node)) return node.children.some((child) => nodeIsActive(child, activeTo));
  return node.to === activeTo;
}

function NodeLabel({ node }: { node: NavGroup | NavLink }): ReactElement {
  const { t } = useTranslation();
  return (
    <>
      <span>{resolveNavLabel(node, t)}</span>
      {node.badge != null && <span className="ml-auto">{node.badge}</span>}
    </>
  );
}

function MobileLink({
  node,
  activeTo,
  close,
  depth,
}: {
  node: NavLink;
  activeTo: string | undefined;
  close: () => void;
  depth: number;
}) {
  const active = nodeIsActive(node, activeTo);
  return (
    <SidebarMenuSubItem>
      <SidebarMenuSubButton
        asChild
        isActive={active}
        className={`h-11 ${depth > 1 ? "pl-8" : "pl-4"}`}
      >
        <Link to={node.to} aria-current={active ? "page" : undefined} onClick={close}>
          <NodeLabel node={node} />
        </Link>
      </SidebarMenuSubButton>
    </SidebarMenuSubItem>
  );
}

function InlineNode({
  node,
  activeTo,
  close,
  depth = 1,
}: {
  node: NavNode;
  activeTo: string | undefined;
  close: () => void;
  depth?: number;
}): ReactElement {
  const { t } = useTranslation();
  if (!isNavGroup(node)) {
    return <MobileLink node={node} activeTo={activeTo} close={close} depth={depth} />;
  }
  const label = resolveNavLabel(node, t);
  const active = nodeIsActive(node, activeTo);
  return (
    <Collapsible asChild defaultOpen={active} className="group/collapsible">
      <SidebarMenuSubItem>
        <CollapsibleTrigger asChild>
          <SidebarMenuSubButton isActive={active} className={depth > 1 ? "pl-8" : "pl-4"}>
            <span>{label}</span>
            {node.badge != null && <span className="ml-auto">{node.badge}</span>}
            <ChevronRight className="ml-auto transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
          </SidebarMenuSubButton>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub>
            {node.children.map((child) => (
              <InlineNode
                key={nodeKey(child)}
                node={child}
                activeTo={activeTo}
                close={close}
                depth={depth + 1}
              />
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuSubItem>
    </Collapsible>
  );
}

function nodeKey(node: NavNode): string {
  return isNavGroup(node) ? `group:${resolveNavLabel(node, (key) => key)}` : `link:${node.to}`;
}

function FlyoutNode({
  node,
  activeTo,
}: {
  node: NavNode;
  activeTo: string | undefined;
}): ReactElement {
  const { t } = useTranslation();
  if (!isNavGroup(node)) {
    const active = nodeIsActive(node, activeTo);
    return (
      <SidebarMenuSubButton asChild isActive={active} className="w-full">
        <Link to={node.to} aria-current={active ? "page" : undefined}>
          <NodeLabel node={node} />
        </Link>
      </SidebarMenuSubButton>
    );
  }
  const active = nodeIsActive(node, activeTo);
  return (
    <Popover>
      <PopoverTrigger asChild>
        <SidebarMenuSubButton
          isActive={active}
          className="w-full justify-between"
          aria-haspopup="menu"
        >
          <span className="truncate">{resolveNavLabel(node, t)}</span>
          <ChevronRight className="ml-2 shrink-0" aria-hidden="true" />
        </SidebarMenuSubButton>
      </PopoverTrigger>
      <PopoverContent
        side="right"
        align="start"
        sideOffset={6}
        collisionPadding={8}
        className="w-56 p-1"
      >
        <p className="px-2 py-1 text-xs font-medium text-muted-foreground">
          {resolveNavLabel(node, t)}
        </p>
        <div className="space-y-1">
          {node.children.map((child) => (
            <FlyoutNode key={nodeKey(child)} node={child} activeTo={activeTo} />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}

export function NavModuleItem({
  module,
  pathname,
}: {
  module: NavModule;
  pathname: string;
}): ReactElement {
  const { state, isMobile, setOpenMobile } = useSidebar();
  const { t } = useTranslation();
  const label = resolveNavLabel(module, t);
  const activeTo = activeNavTarget(module.subItems, pathname);
  const active = activeTo !== undefined;
  const close = () => {
    if (isMobile) setOpenMobile(false);
  };

  if (state === "collapsed" && !isMobile) {
    return (
      <SidebarMenuItem>
        <Popover>
          <PopoverTrigger asChild>
            <SidebarMenuButton isActive={active} tooltip={label}>
              <module.icon />
              <span>{label}</span>
            </SidebarMenuButton>
          </PopoverTrigger>
          <PopoverContent
            side="right"
            align="start"
            sideOffset={8}
            collisionPadding={8}
            className="w-56 p-1"
          >
            <p className="px-2 py-1 text-xs font-medium text-muted-foreground">{label}</p>
            <div className="space-y-1">
              {module.subItems.map((node) => (
                <FlyoutNode key={nodeKey(node)} node={node} activeTo={activeTo} />
              ))}
            </div>
          </PopoverContent>
        </Popover>
      </SidebarMenuItem>
    );
  }

  return (
    <Collapsible asChild defaultOpen={active} className="group/collapsible">
      <SidebarMenuItem>
        <CollapsibleTrigger asChild>
          <SidebarMenuButton isActive={active} tooltip={label} size={isMobile ? "lg" : "default"}>
            <module.icon />
            <span>{label}</span>
            {module.badge != null && <span className="ml-auto">{module.badge}</span>}
            <ChevronRight className="ml-auto transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
          </SidebarMenuButton>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub>
            {module.subItems.map((node) => (
              <InlineNode key={nodeKey(node)} node={node} activeTo={activeTo} close={close} />
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuItem>
    </Collapsible>
  );
}
