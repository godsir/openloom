import React, { useEffect, useRef, useState } from "react";
import { t } from "../../helpers";
import { Toggle } from "../../widgets/Toggle";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsRow } from "../../components/SettingsRow";
import { loomRpc } from "../../../adapter";

interface Props {
  availableTools?: string[];
  disabled: string[];
}

function humanize(name: string): string {
  return name
    .replace(/_/g, " ")
    .replace(/-/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export function AgentToolsSection({ availableTools, disabled }: Props) {
  const [globalDisabled, setGlobalDisabled] = useState<string[]>(disabled);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    loomRpc("config.get", { key: "settings.tools" })
      .then((r: any) => {
        const d = r?.config?.disabled;
        if (Array.isArray(d)) {
          setGlobalDisabled(d);
        }
      })
      .catch(() => {})
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    if (loaded) return;
    setGlobalDisabled(disabled);
  }, [disabled, loaded]);

  const renderable =
    Array.isArray(availableTools) && availableTools.length > 0
      ? availableTools
      : null;

  const normalizedDisabled = globalDisabled.filter((n) =>
    renderable ? renderable.includes(n) : true
  );
  const disabledRef = useRef(normalizedDisabled);
  useEffect(() => {
    disabledRef.current = normalizedDisabled;
  }, [normalizedDisabled]);

  const toggleTool = (name: string) => {
    const current = disabledRef.current;
    const currentlyOff = current.includes(name);
    const newDisabled = currentlyOff
      ? current.filter((n) => n !== name)
      : [...current, name];
    disabledRef.current = newDisabled;
    setGlobalDisabled(newDisabled);
    loomRpc("config.set", {
      key: "settings.tools",
      value: { disabled: newDisabled },
    }).catch(() => {});
  };

  if (renderable !== null && renderable.length === 0) {
    return null;
  }

  return (
    <SettingsSection title={t("settings.agent.tools.title")}>
      <SettingsSection.Note>
        {t("settings.agent.tools.description")}
      </SettingsSection.Note>
      {(renderable || []).map((name) => {
        const isOn = !normalizedDisabled.includes(name);
        return (
          <SettingsRow
            key={name}
            data-tool-name={name}
            label={humanize(name)}
            hint={t(`settings.agent.tools.items.${name}.summary`)}
            control={<Toggle on={isOn} onChange={() => toggleTool(name)} />}
          />
        );
      })}
    </SettingsSection>
  );
}
