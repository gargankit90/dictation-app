import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { AUTO_LABEL, englishName, languageLabel, languageMatches, sortedByName } from "./languages";

/** Where the picker can be: what scrolls around it, in setup and in Settings. */
const SCROLL_AREAS = ".ob-content, .settings-pane";
/** Room kept around the list: its gap below the field, and the area's edge. */
const LIST_MARGIN_PX = 32;
/** Three rows, however short the area is: it scrolls for the rest. */
const MIN_LIST_PX = 96;

interface Option {
  /** The stored value: `""` for automatic, otherwise the ISO code. */
  value: string;
  label: string;
}

interface Props {
  id: string;
  /** The stored value: `""` for automatic, otherwise an ISO code. */
  value: string;
  onChange: (value: string) => void;
  /** The codes the engine can transcribe. */
  supported: readonly string[];
  /** The engine's name, for "not supported by …". `null`: nothing is flagged. */
  engineName: string | null;
  disabled?: boolean;
  /** Ids of the hint and error paragraphs. */
  describedBy?: string;
  invalid?: boolean;
}

/**
 * A searchable popup (ARIA combobox with a listbox): "Detect automatically",
 * then languages by name. A stored code the engine can't do stays selected
 * and is flagged, never changed.
 */
export function LanguagePicker({
  id,
  value,
  onChange,
  supported,
  engineName,
  disabled = false,
  describedBy,
  invalid = false,
}: Props) {
  const listId = useId();
  const field = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const unsupported = value !== "" && engineName !== null && !supported.includes(value);
  const selectedLabel =
    value === ""
      ? AUTO_LABEL
      : unsupported
        ? `${englishName(value)}, not supported by ${engineName}`
        : languageLabel(value);

  const options = useMemo(() => {
    const all: Option[] = [{ value: "", label: AUTO_LABEL }];
    if (unsupported) all.push({ value, label: selectedLabel });
    for (const code of sortedByName(supported)) {
      all.push({ value: code, label: languageLabel(code) });
    }
    const q = query.trim().toLowerCase();
    return all.filter((option) => {
      if (q === "") return true;
      if (option.value === "") return AUTO_LABEL.toLowerCase().includes(q);
      return languageMatches(option.value, q);
    });
  }, [supported, unsupported, value, selectedLabel, query]);

  // The field and the list both show when it opens, in a short window too: the
  // list is only as tall as what is left of the area it scrolls in.
  useEffect(() => {
    const list = document.getElementById(listId);
    const box = field.current;
    if (!open || list === null || box === null) return;
    const area = list.closest(SCROLL_AREAS);
    if (area !== null) {
      const room = area.clientHeight - box.offsetHeight - LIST_MARGIN_PX;
      list.style.maxHeight = `${String(Math.max(room, MIN_LIST_PX))}px`;
    }
    box.scrollIntoView({ block: "nearest" });
    list.scrollIntoView({ block: "nearest" });
  }, [open, listId]);

  // Keep the keyboard-active option visible, moving only the list: an option's
  // own scrollIntoView also scrolls the page and can hide the field.
  useEffect(() => {
    if (!open) return;
    const list = document.getElementById(listId);
    const row = document.getElementById(`${listId}-${String(active)}`);
    if (list === null || row === null) return;
    if (row.offsetTop < list.scrollTop) list.scrollTop = row.offsetTop;
    const below = row.offsetTop + row.offsetHeight - list.clientHeight;
    if (below > list.scrollTop) list.scrollTop = below;
  }, [open, active, listId, options]);

  function close() {
    setOpen(false);
    setQuery("");
  }

  function choose(option: Option) {
    onChange(option.value);
    close();
  }

  function openList() {
    if (disabled || open) return;
    setOpen(true);
    const at = options.findIndex((option) => option.value === value);
    setActive(Math.max(at, 0));
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) openList();
      else if (event.key === "ArrowDown") setActive((i) => Math.min(i + 1, options.length - 1));
      else setActive((i) => Math.max(i - 1, 0));
    } else if (open && event.key === "Home" && query === "") {
      event.preventDefault();
      setActive(0);
    } else if (open && event.key === "End" && query === "") {
      event.preventDefault();
      setActive(options.length - 1);
    } else if (open && event.key === "Enter") {
      event.preventDefault();
      const option = options[active];
      if (option !== undefined) choose(option);
    } else if (open && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Tab") {
      close();
    }
  }

  const activeId =
    open && options[active] !== undefined ? `${listId}-${String(active)}` : undefined;

  return (
    <div className="language-picker">
      <input
        ref={field}
        id={id}
        type="text"
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={activeId}
        aria-describedby={describedBy}
        aria-invalid={invalid}
        autoComplete="off"
        spellCheck={false}
        disabled={disabled}
        value={open ? query : selectedLabel}
        placeholder={open ? "Search languages" : undefined}
        onFocus={openList}
        onClick={openList}
        onBlur={close}
        onChange={(event) => {
          const typed = event.target.value;
          // Closed, the field holds the language's name: only what was added
          // to it is a search.
          if (open) setQuery(typed);
          else {
            setOpen(true);
            setQuery(typed.startsWith(selectedLabel) ? typed.slice(selectedLabel.length) : "");
          }
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      <svg
        className={open ? "language-chevron open" : "language-chevron"}
        aria-hidden="true"
        viewBox="0 0 12 12"
        width="12"
        height="12"
      >
        <path
          d="M2.5 4.5 6 8l3.5-3.5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      {open && (
        <ul id={listId} role="listbox" aria-label="Languages" className="language-list">
          {options.length === 0 && <li className="language-empty">No language matches.</li>}
          {options.map((option, index) => (
            <li
              key={option.value === "" ? "auto" : option.value}
              id={`${listId}-${String(index)}`}
              role="option"
              aria-selected={option.value === value}
              className={index === active ? "language-option active" : "language-option"}
              onMouseDown={(event) => {
                // Keep focus in the input so blur doesn't close the list first.
                event.preventDefault();
                choose(option);
              }}
              onMouseEnter={() => {
                setActive(index);
              }}
            >
              {option.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
