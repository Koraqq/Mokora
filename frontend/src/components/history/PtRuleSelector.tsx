// PT-rule selector: tabs for Majsoul / Custom, with rule-
// specific sub-controls. Stateless wrt the rule itself — reads/writes
// via `useHistoryStore`.
//
// Majsoul's "use current rank" switch hands room and rank to the games
// themselves (`PtRule.auto`); the two selects then show the player's
// current ones (`current`, from `withCurrentRank`) and are read-only.
//
// The header's right side holds the page's view controls (`action`): the
// day, friendly / event and per-bot switches every chart and stats card
// follows.

import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs'
import { DEFAULT_CUSTOM_RULE, type PtRule } from '@/lib/ptCalc'
import {
  MAJSOUL_DAN_4P,
  MAJSOUL_DAN_LABEL,
  MAJSOUL_LOBBY,
  MAJSOUL_LOBBY_LABEL,
  type MajsoulDan,
  type MajsoulLobby,
} from '@/lib/ptTables'
import { useHistoryStore } from '@/stores/historyStore'

export function PtRuleSelector({
  current,
  action,
}: {
  current: PtRule
  /** Header controls on the right: the page-wide view switches. */
  action?: ReactNode
}) {
  const rule = useHistoryStore((s) => s.rule)
  const setRule = useHistoryStore((s) => s.setRule)
  const { t } = useTranslation()

  const onTabChange = (value: string) => {
    if (value === 'majsoul') {
      setRule({ kind: 'majsoul', lobby: 'jade', dan: 'jakketsu_3' })
    } else if (value === 'custom') {
      setRule(DEFAULT_CUSTOM_RULE)
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <CardTitle className="text-sm uppercase tracking-wider">
            {t('history.rule.title')}
          </CardTitle>
          {action}
        </div>
      </CardHeader>
      <CardContent>
        <Tabs value={rule.kind} onValueChange={onTabChange}>
          <div className="flex items-center gap-4 flex-wrap">
            <TabsList>
              <TabsTrigger value="majsoul">{t('history.rule.majsoul')}</TabsTrigger>
              <TabsTrigger value="custom">{t('history.rule.custom')}</TabsTrigger>
            </TabsList>
            {rule.kind === 'majsoul' && (
              <div
                className="flex items-center gap-2"
                title={t('history.rule.use_current_hint')}
              >
                <Switch
                  id="pt-rule-current-rank"
                  checked={!!rule.auto}
                  onCheckedChange={(auto) => setRule({ ...rule, auto })}
                />
                <Label htmlFor="pt-rule-current-rank" className="text-xs">
                  {t('history.rule.use_current')}
                </Label>
              </div>
            )}
          </div>

          <TabsContent value="majsoul" className="pt-3">
            {rule.kind === 'majsoul' && current.kind === 'majsoul' && (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Field label={t('history.rule.lobby')}>
                  <Select
                    value={current.lobby}
                    disabled={rule.auto}
                    onValueChange={(v) =>
                      setRule({ ...rule, lobby: v as MajsoulLobby })
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {MAJSOUL_LOBBY.map((id) => (
                        <SelectItem key={id} value={id}>
                          {MAJSOUL_LOBBY_LABEL[id]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Field label={t('history.rule.dan')}>
                  <Select
                    value={current.dan}
                    disabled={rule.auto}
                    onValueChange={(v) =>
                      setRule({ ...rule, dan: v as MajsoulDan })
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {MAJSOUL_DAN_4P.map((id) => (
                        <SelectItem key={id} value={id}>
                          {MAJSOUL_DAN_LABEL[id]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </Field>
              </div>
            )}
          </TabsContent>

          <TabsContent value="custom" className="pt-3">
            {rule.kind === 'custom' && <CustomEditor />}
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  )
}

function CustomEditor() {
  const rule = useHistoryStore((s) => s.rule)
  const setRule = useHistoryStore((s) => s.setRule)
  const { t } = useTranslation()
  if (rule.kind !== 'custom') return null

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
      <NumberArrayField
        label={t('history.rule.uma_4p')}
        values={rule.uma4p}
        onChange={(vs) =>
          setRule({ ...rule, uma4p: vs as [number, number, number, number] })
        }
      />
      <NumberArrayField
        label={t('history.rule.uma_3p')}
        values={rule.uma3p}
        onChange={(vs) =>
          setRule({ ...rule, uma3p: vs as [number, number, number] })
        }
      />
      <NumberArrayField
        label={t('history.rule.dan_bonus_4p')}
        values={rule.danBonus4p}
        onChange={(vs) =>
          setRule({
            ...rule,
            danBonus4p: vs as [number, number, number, number],
          })
        }
      />
      <NumberArrayField
        label={t('history.rule.dan_bonus_3p')}
        values={rule.danBonus3p}
        onChange={(vs) =>
          setRule({ ...rule, danBonus3p: vs as [number, number, number] })
        }
      />
    </div>
  )
}

function NumberArrayField({
  label,
  values,
  onChange,
}: {
  label: string
  values: number[]
  onChange: (vs: number[]) => void
}) {
  return (
    <div className="space-y-1">
      <Label className="text-xs">{label}</Label>
      <div className="flex gap-1">
        {values.map((v, i) => (
          <Input
            key={i}
            type="number"
            value={v}
            onChange={(e) => {
              const n = parseFloat(e.target.value)
              if (!Number.isFinite(n)) return
              const next = [...values]
              next[i] = n
              onChange(next)
            }}
            className="w-full"
          />
        ))}
      </div>
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <Label className="text-xs">{label}</Label>
      {children}
    </div>
  )
}
