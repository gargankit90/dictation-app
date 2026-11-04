import { useId, useMemo, useState } from "react";
import type { KeyboardEvent } from "react";
import { AUTO_LABEL, englishName, languageLabel, languageMatches, sortedByName } from "./languages";

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
          if (!open) setOpen(true);
          setQuery(event.target.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
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
