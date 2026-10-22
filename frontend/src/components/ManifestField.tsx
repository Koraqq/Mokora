import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { FieldSpec } from '@/types'

export function ManifestField({
  fieldKey,
  spec,
  value,
  onChange,
}: {
  fieldKey: string
  spec: FieldSpec
  value: unknown
  onChange: (v: unknown) => void
}) {
  return (
    <div className="grid gap-1.5">
      <Label>{spec.label}</Label>
      {renderInput(fieldKey, spec, value, onChange)}
      {spec.help && <span className="text-xs text-muted-foreground">{spec.help}</span>}
    </div>
  )
}

function renderInput(
  _key: string,
  spec: FieldSpec,
  value: unknown,
  onChange: (v: unknown) => void,
) {
  switch (spec.type) {
    case 'bool':
      return <Switch checked={Boolean(value)} onCheckedChange={onChange} />
    case 'enum':
      return (
        <Select value={String(value ?? '')} onValueChange={onChange}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {(spec.choices ?? []).map((c) => (
              <SelectItem key={c} value={c}>
                {c}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )
    case 'int':
    case 'float':
      return (
        <Input
          type="number"
          value={value == null ? '' : String(value)}
          min={spec.min}
          max={spec.max}
          step={spec.step ?? (spec.type === 'int' ? 1 : 'any')}
          onChange={(e) => {
            const v = e.target.value
            if (v === '') onChange(null)
            else onChange(spec.type === 'int' ? parseInt(v, 10) : parseFloat(v))
          }}
        />
      )
    default:
      return (
        <Input
          type={spec.secret ? 'password' : 'text'}
          value={String(value ?? '')}
          onChange={(e) => onChange(e.target.value)}
        />
      )
  }
}
