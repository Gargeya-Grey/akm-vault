interface Option {
  value: string;
  label: string;
}

interface Props {
  id: string;
  label: string;
  value: string;
  placeholder: string;
  options: Option[];
  onChange: (value: string) => void;
  disabled?: boolean;
}

/// A labelled filter select. The caret is drawn in CSS, not an image, so a
/// strict CSP does not break it.
export function FilterSelect({
  id,
  label,
  value,
  placeholder,
  options,
  onChange,
  disabled = false,
}: Props) {
  return (
    <div className="filter">
      <label className="label" htmlFor={id}>{label}</label>
      <select
        id={id}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{placeholder}</option>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}

export function uniqueOptions(values: (string | null)[]): Option[] {
  const seen = new Set<string>();
  for (const value of values) {
    if (value) seen.add(value);
  }
  return [...seen]
    .sort((a, b) => a.localeCompare(b))
    .map((value) => ({ value, label: value }));
}
