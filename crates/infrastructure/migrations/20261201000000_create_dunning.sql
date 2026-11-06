-- Reminders to customers about the installments of their receivables (ADR 0048): a plan per company with
-- its rules, and the log of what was sent. The constraints repeat the rules of the domain types that one
-- row can show; that a plan has at most 20 rules is the domain's.

CREATE TABLE dunning_plans (
    company_id UUID PRIMARY KEY REFERENCES companies (id),
    enabled    BOOLEAN NOT NULL
);

CREATE TABLE dunning_rules (
    company_id UUID     NOT NULL,
    position   SMALLINT NOT NULL,
    -- Negative before the due date, 0 on it, positive after it.
    days       SMALLINT NOT NULL,
    channel    TEXT     NOT NULL,
    PRIMARY KEY (company_id, position),
    CONSTRAINT dunning_rules_plan_fk FOREIGN KEY (company_id) REFERENCES dunning_plans (company_id),
    CONSTRAINT dunning_rules_position_valid CHECK (position BETWEEN 1 AND 20),
    CONSTRAINT dunning_rules_days_valid CHECK (days BETWEEN -60 AND 180),
    CONSTRAINT dunning_rules_channel_valid CHECK (channel IN ('email', 'whatsapp')),
    CONSTRAINT dunning_rules_once UNIQUE (company_id, days, channel)
);

-- What was sent, once per installment, rule days and channel: the unique key is what makes the hourly pass
-- safe to repeat. A log and not a register: not audited.
CREATE TABLE notifications (
    id          UUID PRIMARY KEY,
    company_id  UUID        NOT NULL,
    entry_id    UUID        NOT NULL,
    installment SMALLINT    NOT NULL,
    days        SMALLINT    NOT NULL,
    channel     TEXT        NOT NULL,
    -- The address or phone it went to.
    recipient   TEXT        NOT NULL,
    status      TEXT        NOT NULL,
    attempts    SMALLINT    NOT NULL,
    note        TEXT,
    -- The order they were made in, which a list keeps.
    seq         BIGSERIAL   NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT notifications_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT notifications_entry_fk FOREIGN KEY (company_id, entry_id)
        REFERENCES financial_entries (company_id, id),
    CONSTRAINT notifications_once UNIQUE (company_id, entry_id, installment, days, channel),
    CONSTRAINT notifications_installment_valid CHECK (installment BETWEEN 1 AND 120),
    CONSTRAINT notifications_days_valid CHECK (days BETWEEN -60 AND 180),
    CONSTRAINT notifications_channel_valid CHECK (channel IN ('email', 'whatsapp')),
    CONSTRAINT notifications_recipient_valid CHECK (
        char_length(recipient) BETWEEN 1 AND 200 AND recipient !~ '[[:cntrl:]]'
    ),
    CONSTRAINT notifications_status_valid CHECK (status IN ('pending', 'sent', 'failed')),
    CONSTRAINT notifications_attempts_valid CHECK (attempts BETWEEN 1 AND 3),
    CONSTRAINT notifications_note_valid CHECK (
        note IS NULL OR (char_length(note) BETWEEN 1 AND 200 AND note !~ '[[:cntrl:]]')
    )
);

CREATE INDEX notifications_entry_idx ON notifications (company_id, entry_id, seq);
