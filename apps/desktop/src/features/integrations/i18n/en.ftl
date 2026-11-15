## Integrations: API access, connection dialogs, API progress, APIs mode (M2-U26)

api-ga4 = Google Analytics 4
api-gsc = Search Console
api-psi = PageSpeed Insights
api-majestic = Majestic
api-ahrefs = Ahrefs
api-moz = Moz
api-drive = Google Drive

integ-short-ga4 = GA4
integ-short-gsc = GSC
integ-short-psi = PSI
integ-short-majestic = Majestic
integ-short-ahrefs = Ahrefs
integ-short-moz = Moz
integ-short-drive = Drive

integ-title = API access
integ-description = Connect data sources and choose what they deliver. Keys and tokens stay in the system keychain.
integ-list = Integrations
integ-source-sections = Settings for { $source }
integ-tab-account = Account
integ-tab-metrics = Metrics
integ-tab-advanced = Advanced
integ-open = API access…
integ-gear = Configure { $source }

integ-status-disconnected = Not connected
integ-status-connecting = Connecting…
integ-status-connected = Connected
integ-status-connected-as = Connected ({ $account })
integ-status-error = Error: { $message }
integ-error-unknown = unknown cause

integ-connect = Connect
integ-connect-saved = Use saved key
integ-connect-account = Connect as { $account }
integ-connect-google = Connect a new Google account
integ-connect-oauth = Sign in with { $source }
integ-connecting = Checking the connection…
integ-connecting-browser = Finish the sign-in in your browser.
integ-disconnect = Disconnect
integ-remove-account = Remove account
integ-api-key = API key
integ-remember = Remember key
integ-saved-keys = Saved keys
integ-google-accounts = Remembered Google accounts
integ-client-secret = Client secret (own Google Cloud client)
integ-client-secret-hint = Only needed for your own OAuth client. It is passed to the sign-in and never written to a profile.
integ-enabled = Fetch data from this source during crawls

integ-catalog-loading = Loading properties…
integ-catalog-unavailable = The property list cannot be loaded here yet. Enter the identifier manually.
integ-catalog-error = The property list could not be loaded. Enter the identifier manually or try again later.
integ-ga4-account = Account
integ-ga4-property = Property
integ-ga4-property-id = GA4 property ID
integ-ga4-property-search = Search properties
integ-ga4-stream = Data stream
integ-ga4-all-streams = All data streams
integ-ga4-no-properties = No GA4 properties found for { $account }.
integ-gsc-property = Property
integ-gsc-property-id = Search Console property (sc-domain:… or URL prefix)
integ-gsc-kind-domain = Domain
integ-gsc-kind-prefix = URL prefix
integ-gsc-unverified = not verified
integ-gsc-no-properties = This Google account has no Search Console properties.
integ-gsc-mismatch = The selected property does not match the crawl domain.

integ-psi-source = Source
integ-psi-remote = Remote (PageSpeed Insights API)
integ-psi-local = Local (Lighthouse on this computer)
integ-psi-remote-hint = Runs Lighthouse on Google servers and adds field data from CrUX. Needs a free API key with the PageSpeed Insights API enabled.
integ-psi-local-hint = Runs Lighthouse on this computer without a key or daily limit; field data from CrUX is not available.
integ-psi-denied = PSI access denied ({ $reason })
integ-psi-checklist = Please check:
integ-psi-check-enabled = Is the PageSpeed Insights API enabled in the Google Cloud project that owns the key?
integ-psi-check-new = Was the key created just now? Wait a few minutes and connect again.
integ-psi-check-restricted = Is the key restricted to certain APIs or referrers?

integ-levels = Levels
integ-level-not-offered = Not offered by this provider
integ-level-not-available = Not available yet
integ-metrics = Metrics
integ-columns = Resulting columns
integ-columns-count = { $count ->
    [one] 1 column
   *[other] { $count } columns
}

integ-concurrency = Concurrent requests
integ-concurrency-hint = Default 5
integ-retries = Retries
integ-range = Allowed: { $min } to { $max }
integ-client-id = Own OAuth client ID
integ-client-id-hint = Client ID of your Google Cloud project (desktop app).
integ-client-id-note = Until a shared client ID ships, Google sign-in needs your own client ID.

integ-invalidate-question = Settings that change the data of { $sources } were modified. Delete the existing data and reload it, or keep it?
integ-invalidate-reload = Delete and reload
integ-invalidate-keep = Keep

integ-progress-name = API progress
integ-progress-label = API: { $percent } %
integ-progress-finished = done
integ-progress-part = { $name } { $amount } ({ $errors ->
    [one] 1 error
   *[other] { $errors } errors
})

integ-apimode = APIs mode
integ-apimode-urls = URLs (one per line)
integ-apimode-file = Load file…
integ-apimode-start = Fetch API data
integ-apimode-count = { $count ->
    [one] 1 URL
   *[other] { $count } URLs
}, { $duplicates } duplicates removed
integ-apimode-preview = { $valid } valid, { $invalid } invalid, { $duplicates } duplicates
integ-apimode-sources = Data from: { $sources }
integ-apimode-none-connected = No integration is connected.
integ-apimode-no-integration = Connect at least one integration first. Nothing was started.
integ-apimode-empty = Add at least one URL.
integ-apimode-started = Fetching API data for { $count } URLs; the URLs themselves are not requested.

integ-menu-api-access = API Access
integ-menu-overview = All integrations…
integ-menu-mode-apis = APIs

err-api-connect-timeout = timed out after { $seconds } s
err-api-test-timeout = The connection test timed out.
err-api-reconnect = The sign-in was rejected or has expired. Please connect again.
err-api-disconnected = The integration is not connected.
err-api-source-unavailable = This data source is not available yet.
err-api-request = The request to the provider failed.
err-api-key = Please enter an API key.
err-api-lighthouse-missing = Lighthouse runtime not found
err-api-client-id = Enter your own OAuth client ID under Advanced first.
err-api-connecting = A connection attempt is already running.
err-api-account = The selected account no longer exists.
err-api-kind = The settings belong to a different integration.
err-api-settings = The settings are invalid.
err-api-secret-in-settings = Keys and tokens cannot be stored in settings.
notice-api-session-only = The keychain is not available; the key is used for this session only.
notice-api-revoke-failed = The account was removed locally, but the provider could not revoke the access.
op-api-connect = Connecting integration
op-api-fetch = Fetching API data
