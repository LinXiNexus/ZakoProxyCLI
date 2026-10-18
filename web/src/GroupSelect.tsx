import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { Check, ChevronDown, FolderClosed } from "lucide-react";

export function GroupSelect({
  value,
  groups,
  onChange,
  disabled = false,
}: {
  value: string;
  groups: { id: string; name: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const search = useRef({ text: "", time: 0 });
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const selected = groups.find((group) => group.id === value);
  const unavailable = disabled || !groups.length;

  function positionMenu() {
    const button = trigger.current;
    const popup = menu.current;
    if (!button || !popup) return;
    const rect = button.getBoundingClientRect();
    const viewport = window.visualViewport;
    const viewTop = viewport?.offsetTop ?? 0;
    const viewLeft = viewport?.offsetLeft ?? 0;
    const viewHeight = viewport?.height ?? window.innerHeight;
    const viewWidth = viewport?.width ?? window.innerWidth;
    const below = viewTop + viewHeight - rect.bottom - 14;
    const above = rect.top - viewTop - 14;
    const desiredHeight = Math.min(280, groups.length * 38 + 42);
    const upward = below < desiredHeight && above > below;
    const height = Math.min(desiredHeight, Math.max(60, upward ? above : below));
    const width = Math.min(rect.width, viewWidth - 24);
    popup.style.width = `${width}px`;
    popup.style.maxHeight = `${height}px`;
    popup.style.left = `${Math.max(viewLeft + 12, Math.min(rect.left, viewLeft + viewWidth - width - 12))}px`;
    popup.style.top = `${upward ? Math.max(viewTop + 12, rect.top - height - 6) : rect.bottom + 6}px`;
  }

  function close() {
    menu.current?.hidePopover();
    setOpen(false);
  }

  function show(index = Math.max(0, groups.findIndex((group) => group.id === value))) {
    if (unavailable) return;
    setActive(index);
    positionMenu();
    menu.current?.showPopover();
    setOpen(true);
  }

  function choose(index: number) {
    const group = groups[index];
    if (!group) return;
    onChange(group.id);
    close();
    trigger.current?.focus();
  }

  useEffect(() => {
    if (!open) return;
    const reposition = () => positionMenu();
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    window.visualViewport?.addEventListener("resize", reposition);
    window.visualViewport?.addEventListener("scroll", reposition);
    return () => {
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
      window.visualViewport?.removeEventListener("resize", reposition);
      window.visualViewport?.removeEventListener("scroll", reposition);
    };
  }, [open, groups.length]);

  useEffect(() => {
    if (open) menu.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  useEffect(() => {
    if (unavailable && open) close();
  }, [unavailable, open]);

  function onKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (unavailable) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) show();
      else setActive((index) => (index + (event.key === "ArrowDown" ? 1 : -1) + groups.length) % groups.length);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const index = event.key === "Home" ? 0 : groups.length - 1;
      if (!open) show(index);
      else setActive(index);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) choose(active);
      else show();
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Tab") {
      close();
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const now = Date.now();
      const text = (now - search.current.time < 700 ? search.current.text : "") + event.key.toLocaleLowerCase();
      search.current = { text, time: now };
      const index = groups.findIndex((group) => group.name.toLocaleLowerCase().startsWith(text));
      if (index >= 0) {
        event.preventDefault();
        if (open) setActive(index);
        else show(index);
      }
    }
  }

  return (
    <div className="field">
      <label id={`${id}-label`} htmlFor={`${id}-trigger`} className="field-label">所属分组</label>
      <span className="group-select">
      <button
        id={`${id}-trigger`}
        ref={trigger}
        type="button"
        className={`group-select-trigger${open ? " is-open" : ""}${!selected ? " is-placeholder" : ""}`}
        role="combobox"
        aria-labelledby={`${id}-label`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${id}-list`}
        aria-activedescendant={open ? `${id}-option-${active}` : undefined}
        aria-required="true"
        disabled={unavailable}
        onKeyDown={onKeyDown}
        onClick={() => open ? close() : show()}
      >
        <FolderClosed size={15} aria-hidden="true" />
        <span>{selected?.name ?? (groups.length ? "选择分组" : "请先创建分组")}</span>
        <ChevronDown size={15} className="group-select-chevron" aria-hidden="true" />
      </button>
      <div
        ref={menu}
        id={`${id}-list`}
        className="group-select-menu"
        popover="auto"
        role="listbox"
        aria-label="选择所属分组"
        onToggle={(event) => setOpen(event.currentTarget.matches(":popover-open"))}
      >
        <div className="group-select-heading" role="presentation">选择分组<span>{groups.length} 个分组</span></div>
        {groups.map((group, index) => (
          <div
            id={`${id}-option-${index}`}
            key={group.id}
            role="option"
            aria-selected={group.id === value}
            data-index={index}
            className={`group-select-option${index === active ? " is-active" : ""}`}
            onPointerMove={() => setActive(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(index)}
          >
            <FolderClosed size={14} aria-hidden="true" />
            <span title={group.name}>{group.name}</span>
            {group.id === value && <Check size={15} strokeWidth={2.3} aria-hidden="true" />}
          </div>
        ))}
      </div>
      </span>
    </div>
  );
}
