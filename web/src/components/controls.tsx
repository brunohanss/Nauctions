import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
} from "./ui/accordion";
import { Badge } from "./ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { Pagination } from "./ui/pagination";
import React, {
  Children,
  isValidElement,
  type SelectHTMLAttributes,
  type ReactNode,
  useState,
  useEffect,
  type DetailsHTMLAttributes,
} from "react";
import {
  Select as Root,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "./ui/select";
import { Checkbox } from "./ui/checkbox";
import { Switch } from "./ui/switch";
import { Input as TextInput } from "./ui/input";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from "./ui/collapsible";
import { Button as PrimitiveButton } from "./ui/button";
export function Button(props: React.ComponentProps<typeof PrimitiveButton>) {
  return <PrimitiveButton variant="outline" {...props} />;
}
export function Input(props: React.ComponentProps<typeof TextInput>) {
  if (props.type !== "checkbox") return <TextInput {...props} />;
  const Control =
    props.id === "desktop-startup" || props.id === "desktop-automatic"
      ? Switch
      : Checkbox;
  return (
    <Control
      id={props.id}
      checked={props.checked}
      disabled={props.disabled}
      aria-label={props["aria-label"]}
      onCheckedChange={(checked) =>
        props.onChange?.({
          target: { checked: checked === true },
        } as React.ChangeEvent<HTMLInputElement>)
      }
    />
  );
}
export { Label } from "./ui/label";
export {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "./ui/table";
export function Select({
  children,
  value,
  defaultValue,
  onChange,
  className,
  id,
  disabled,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement>) {
  const options = Children.toArray(children).filter(
    isValidElement,
  ) as React.ReactElement<{ value: string; children: ReactNode }>[];
  const encode = (v: unknown) => String(v ?? "") || "__all__";
  return (
    <Root
      value={value === undefined ? undefined : encode(value)}
      defaultValue={
        defaultValue === undefined ? undefined : encode(defaultValue)
      }
      disabled={disabled}
      name={props.name}
      onValueChange={(v) =>
        onChange?.({
          target: { value: v === "__all__" ? "" : v },
        } as React.ChangeEvent<HTMLSelectElement>)
      }
    >
      <SelectTrigger
        id={id}
        className={className}
        aria-label={props["aria-label"]}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper" className="z-[80]">
        {options.map((o) => (
          <SelectItem key={encode(o.props.value)} value={encode(o.props.value)}>
            {o.props.children}
          </SelectItem>
        ))}
      </SelectContent>
    </Root>
  );
}
function hasAlert(nodes: ReactNode): boolean {
  return Children.toArray(nodes).some(
    (node) =>
      isValidElement<{ role?: string; children?: ReactNode }>(node) &&
      (node.props.role === "alert" || hasAlert(node.props.children)),
  );
}
export function Details({
  children,
  className,
  open,
}: DetailsHTMLAttributes<HTMLDetailsElement>) {
  const nodes = Children.toArray(children);
  const summary = nodes.find(
    (n) => isValidElement(n) && n.type === "summary",
  ) as React.ReactElement<{ children: ReactNode }> | undefined;
  const content = nodes.filter((n) => n !== summary);
  const issue = hasAlert(content);
  const [expanded, setExpanded] = useState(!!open || issue);
  useEffect(() => {
    if (issue || open) setExpanded(true);
  }, [issue, open]);
  if (className?.includes("settings-section"))
    return (
      <Accordion
        type="single"
        collapsible
        value={expanded ? "section" : ""}
        onValueChange={(v) => setExpanded(!!v)}
        className={className}
      >
        <AccordionItem value="section">
          <AccordionTrigger>
            {summary?.props.children}
            {issue && <Badge variant="outline">Needs attention</Badge>}
          </AccordionTrigger>
          <AccordionContent forceMount hidden={!expanded}>
            {content}
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    );
  return (
    <Collapsible
      open={expanded}
      onOpenChange={setExpanded}
      className={className}
    >
      <CollapsibleTrigger className="details-trigger">
        {summary?.props.children}
      </CollapsibleTrigger>
      <CollapsibleContent forceMount className="details-content">
        {content}
      </CollapsibleContent>
    </Collapsible>
  );
}
export function Help({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" className="help-trigger" aria-label={label}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function Pager({ children }: { children: ReactNode }) {
  return (
    <Pagination className="pagination w-auto mx-0" aria-label="Results pages">
      {children}
    </Pagination>
  );
}
