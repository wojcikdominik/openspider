ai-settings-title = AI settings
settings-section-ai = AI
config-page-ai = AI

## AI section of the app settings
ai-settings-tab-costs = Cost and privacy
ai-settings-tab-dataflow = AI data flow
ai-settings-tab-advanced = Advanced
ai-settings-tab-accounts = Accounts
ai-settings-apply = Apply
ai-settings-saved = Saved
ai-settings-usd-invalid = Please enter an amount in USD, e.g. 1.00.
ai-settings-threshold = Ask for confirmation from (USD expected cost)
ai-settings-threshold-hint = If the cost estimate is above this value or above half of the remaining budget, no AI request starts without "Start".
ai-settings-eu-title = EU processing
ai-settings-eu-hint = Under the policy "Enforce EU processing" only providers with the EU endpoint switched on and a confirmed account are allowed.
ai-settings-eu-use = Use the EU endpoint ({ $url })
ai-settings-eu-confirmed = Account or project is enabled for EU processing
ai-settings-eu-not-allowed = Currently not allowed under "Enforce EU processing".
ai-settings-eu-anthropic = Anthropic offers no EU endpoint; under "Enforce EU processing" Anthropic tasks get "Skipped (policy)".
ai-settings-redaction-title = Redaction
ai-settings-redaction-switch = Redact personal data before sending
ai-settings-patterns = Own patterns (one regular expression per line)
ai-settings-patterns-save = Save patterns
ai-settings-pattern-invalid = Invalid pattern in line { $line }, position { $position }.
ai-settings-advanced-hint = Concurrent requests per AI provider. Providers with their own default show that default.
ai-settings-concurrency = Concurrent requests to { $provider }
ai-settings-concurrency-hint = Default: { $standard } · range { $min }–{ $max }
ai-settings-concurrency-invalid = Please enter a whole number from { $min } to { $max }.
ai-settings-accounts-later = Keys and the connection test of the AI providers follow in a later version of the preview.

## Configuration page AI: model choice per task (AI-011)
ai-tasks-title = Model choice per task
ai-tasks-hint = One model with a fallback list per task type. The reasoning level applies to the task type, not to single questions.
ai-tasks-error = The model choice could not be checked.
ai-col-task = Task
ai-col-model = Model
ai-col-reasoning = Reasoning
ai-col-max-tokens = Output limit
ai-col-fallback = Fallback list
ai-col-status = Status
ai-col-actions = Actions
ai-task-model = Model for { $task }
ai-task-model-default = Default ({ $model })
ai-task-no-default = no default model
ai-task-unassignable = Not assignable: { $models }
ai-task-incapable = The model lacks a required capability: { $missing }
ai-task-reasoning = Reasoning level for { $task }
ai-task-reasoning-default = Task default ({ $level })
ai-task-effective = Effective: { $level }
ai-task-levels-unknown = Levels of the model not known yet (model list not loaded).
ai-task-no-level = no level
ai-task-warn-off = Switching off is not available for this model – the lowest level applies: { $effective }.
ai-task-warn-stored = The stored level { $stored } is not available for this model; { $effective } applies. The stored value is kept.
ai-task-max-tokens = Output limit for { $task } (empty = default)
ai-task-max-standard = Default: { $standard } tokens
ai-task-max-formula = Default: 2 × (64 + 16 × W) + reserve of the level
ai-task-max-tight = Output limit tight – answers may stop (Error: Output limit reached)
ai-task-fallback-list = Fallback list for { $task }
ai-task-fallback-add = Add fallback model
ai-task-fallback-up = Move { $model } up
ai-task-fallback-down = Move { $model } down
ai-task-fallback-remove = Remove { $model }
ai-task-reset = Reset to default

ai-level-model-default = model default
ai-level-off = off
ai-level-minimal = minimal
ai-level-low = low
ai-level-medium = medium
ai-level-high = high
ai-level-xhigh = xhigh
ai-level-max = max

## Policy (AI-019) and crawl budget (AI-013)
ai-policy-title = Policy for AI providers
ai-policy-label = Where may data go?
ai-policy-unrestricted = No restriction
ai-policy-eu-only = Enforce EU processing
ai-policy-local-only = Local only
ai-policy-unrestricted-hint = All connected providers are allowed.
ai-policy-eu-only-hint = Only endpoints with processing in the EU; otherwise "Skipped (policy)". No silent fallback to the global endpoint.
ai-policy-not-enforced = Preview: runs do not enforce the policy yet (EU processing and local only follow with release 1.2). Until then data goes to the chosen provider, even under "Enforce EU processing" or "Local only".
ai-policy-local-only-hint = Local models only (Ollama); no AI request leaves this computer.
ai-budget-title = Crawl budget for paid APIs
ai-budget-crawl = Budget per crawl (USD)
ai-budget-crawl-hint = Applies to all paid calls of this crawl; 0 means no paid calls.
ai-budget-unlimited = Unlimited
ai-budget-unlimited-warning = Without a budget there is no upper limit for the cost of this crawl.
ai-budget-ai-total = Sub-budget "AI total" (USD, empty = none)
ai-budget-ai-total-hint = Limits only the AI tasks within the crawl budget.
ai-budget-warning = Warning threshold (% of the crawl budget)
ai-budget-warning-invalid = Please enter a whole number from 0 to 100.
