import Database from 'better-sqlite3';
import type { Platform, InstanceConfig, IMSettings } from './types';
import { DEFAULT_IM_SETTINGS } from './types';
import type { AccessMode } from './types';

/** Safe JSON.parse: returns fallback value on any parse error. */
function safeJsonParse<T>(raw: string, fallback: T): T {
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export class IMStore {
  private db: Database.Database;

  constructor(db: Database.Database) {
    this.db = db;
    this.ensureTables();
  }

  private ensureTables(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS im_instances (
        id TEXT PRIMARY KEY,
        platform TEXT NOT NULL,
        instance_id TEXT NOT NULL,
        instance_name TEXT NOT NULL DEFAULT '',
        enabled INTEGER NOT NULL DEFAULT 0,
        config_json TEXT NOT NULL DEFAULT '{}',
        dm_policy TEXT NOT NULL DEFAULT 'pairing',
        allow_from TEXT NOT NULL DEFAULT '[]',
        group_policy TEXT NOT NULL DEFAULT 'disabled',
        group_allow_from TEXT NOT NULL DEFAULT '[]',
        agent_id TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
        UNIQUE(platform, instance_id)
      );

      CREATE TABLE IF NOT EXISTS im_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `);

    // im_conversations: migrate old table (without instance_id) to new schema.
    const cols = this.db.prepare('PRAGMA table_info(im_conversations)').all() as any[];
    if (cols.length > 0 && !cols.some((c: any) => c.name === 'instance_id')) {
      this.db.exec('DROP TABLE im_conversations');
    }
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS im_conversations (
        id TEXT PRIMARY KEY,
        platform TEXT NOT NULL,
        instance_id TEXT NOT NULL DEFAULT '',
        conversation_id TEXT NOT NULL,
        cowork_session_id TEXT,
        agent_id TEXT DEFAULT 'main',
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        last_active_at INTEGER NOT NULL DEFAULT (unixepoch()),
        UNIQUE(instance_id, conversation_id)
      );
    `);
  }

  listInstances(): InstanceConfig[] {
    const stmt = this.db.prepare(
      `SELECT id, platform, instance_id, instance_name, enabled, config_json,
              dm_policy, allow_from, group_policy, group_allow_from,
              agent_id, created_at, updated_at
       FROM im_instances ORDER BY platform, instance_id`
    );
    return stmt.all().map((row: any) => ({
      id: row.id,
      platform: row.platform as Platform,
      instanceId: row.instance_id,
      instanceName: row.instance_name,
      enabled: row.enabled !== 0,
      configJson: safeJsonParse(row.config_json, {}),
      dmPolicy: row.dm_policy as AccessMode,
      allowFrom: safeJsonParse(row.allow_from, []),
      groupPolicy: row.group_policy as 'open' | 'allowlist' | 'disabled',
      groupAllowFrom: safeJsonParse(row.group_allow_from, []),
      agentId: row.agent_id || undefined,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }));
  }

  upsertInstance(config: InstanceConfig): void {
    this.db.prepare(
      `INSERT INTO im_instances (id, platform, instance_id, instance_name, enabled,
         config_json, dm_policy, allow_from, group_policy, group_allow_from,
         agent_id, created_at, updated_at)
       VALUES (@id, @platform, @instance_id, @instance_name, @enabled,
         @config_json, @dm_policy, @allow_from, @group_policy, @group_allow_from,
         @agent_id, @created_at, @updated_at)
       ON CONFLICT(platform, instance_id) DO UPDATE SET
         instance_name = excluded.instance_name,
         enabled = excluded.enabled,
         config_json = excluded.config_json,
         dm_policy = excluded.dm_policy,
         allow_from = excluded.allow_from,
         group_policy = excluded.group_policy,
         group_allow_from = excluded.group_allow_from,
         agent_id = excluded.agent_id,
         updated_at = excluded.updated_at`
    ).run({
      id: config.id,
      platform: config.platform,
      instance_id: config.instanceId,
      instance_name: config.instanceName,
      enabled: config.enabled ? 1 : 0,
      config_json: JSON.stringify(config.configJson),
      dm_policy: config.dmPolicy,
      allow_from: JSON.stringify(config.allowFrom),
      group_policy: config.groupPolicy,
      group_allow_from: JSON.stringify(config.groupAllowFrom),
      agent_id: config.agentId || null,
      created_at: config.createdAt,
      updated_at: config.updatedAt,
    });
  }

  deleteInstance(platform: Platform, instanceId: string): void {
    this.db.prepare(
      'DELETE FROM im_instances WHERE platform = ? AND instance_id = ?'
    ).run(platform, instanceId);
  }

  getSettings(): IMSettings {
    const stmt = this.db.prepare('SELECT key, value FROM im_settings');
    const rows = stmt.all() as Array<{ key: string; value: string }>;
    const map: Record<string, string> = {};
    for (const row of rows) {
      map[row.key] = row.value;
    }
    return {
      globalEnabled: map['globalEnabled'] !== undefined ? map['globalEnabled'] === 'true' : DEFAULT_IM_SETTINGS.globalEnabled,
      defaultDmPolicy: (map['defaultDmPolicy'] as AccessMode) || DEFAULT_IM_SETTINGS.defaultDmPolicy,
      skillsEnabled: map['skillsEnabled'] !== undefined ? map['skillsEnabled'] === 'true' : DEFAULT_IM_SETTINGS.skillsEnabled,
      defaultAgentId: map['defaultAgentId'] || DEFAULT_IM_SETTINGS.defaultAgentId,
    };
  }

  setSettings(settings: Partial<IMSettings>): void {
    const upsert = this.db.prepare(
      `INSERT INTO im_settings (key, value) VALUES (@key, @value)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value`
    );
    const tx = this.db.transaction(() => {
      if (settings.globalEnabled !== undefined) upsert.run({ key: 'globalEnabled', value: String(settings.globalEnabled) });
      if (settings.defaultDmPolicy !== undefined) upsert.run({ key: 'defaultDmPolicy', value: settings.defaultDmPolicy });
      if (settings.skillsEnabled !== undefined) upsert.run({ key: 'skillsEnabled', value: String(settings.skillsEnabled) });
      if (settings.defaultAgentId !== undefined) upsert.run({ key: 'defaultAgentId', value: settings.defaultAgentId });
    });
    tx();
  }

  getConversation(instanceId: string, conversationId: string): { coworkSessionId?: string } | null {
    const row = this.db.prepare(
      'SELECT cowork_session_id FROM im_conversations WHERE instance_id = ? AND conversation_id = ?'
    ).get(instanceId, conversationId) as any;
    return row ? { coworkSessionId: row.cowork_session_id } : null;
  }

  /** List conversation ids bound to an instance, most recently active first. */
  listConversations(instanceId: string): Array<{ conversationId: string }> {
    const rows = this.db.prepare(
      'SELECT conversation_id FROM im_conversations WHERE instance_id = ? ORDER BY last_active_at DESC'
    ).all(instanceId) as Array<{ conversation_id: string }>;
    return rows.map((r) => ({ conversationId: r.conversation_id }));
  }

  upsertConversation(platform: Platform, instanceId: string, conversationId: string, coworkSessionId: string): void {
    const id = `${instanceId}:${conversationId}`;
    this.db.prepare(
      `INSERT INTO im_conversations (id, platform, instance_id, conversation_id, cowork_session_id, last_active_at)
       VALUES (@id, @platform, @instance_id, @conversation_id, @cowork_session_id, unixepoch())
       ON CONFLICT(instance_id, conversation_id) DO UPDATE SET
         cowork_session_id = excluded.cowork_session_id,
         last_active_at = unixepoch()`
    ).run({ id, platform, instance_id: instanceId, conversation_id: conversationId, cowork_session_id: coworkSessionId });
  }

  listAllBindings(): Array<{ sessionId: string; platform: Platform; instanceId: string; conversationId: string }> {
    const rows = this.db.prepare(
      'SELECT cowork_session_id, platform, instance_id, conversation_id FROM im_conversations WHERE cowork_session_id IS NOT NULL'
    ).all() as Array<{ cowork_session_id: string; platform: string; instance_id: string; conversation_id: string }>;
    return rows.map(r => ({
      sessionId: r.cowork_session_id,
      platform: r.platform as Platform,
      instanceId: r.instance_id,
      conversationId: r.conversation_id,
    }));
  }
}
