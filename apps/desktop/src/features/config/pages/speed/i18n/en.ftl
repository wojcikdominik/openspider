# Speed page (M2-U13)
config-speed-desc = The thread count takes effect immediately, even while the crawl is running. All other speed settings need a pause.
config-sec-speed-parallel = Parallel requests
config-sec-speed-rate = Request rate
config-f-speed-threads = Max. threads
config-f-speed-max-per-host = Max. simultaneous connections per host
config-f-speed-connections-per-host = Connections per host
config-e-speed-connections-per-host-auto = Automatic (as many as threads, at most 6)
config-hint-connections-per-host = Separate TCP connections per host. With HTTP/1.1 each connection carries one request at a time.
config-f-speed-auto = Adjust the thread count automatically
config-f-speed-max-urls-per-sec = Max. URLs per second
config-f-speed-adaptive-throttle = Slow down automatically when the server is overloaded
config-hint-rate = Decimal numbers are allowed, for example 0.5 for one request every 2 seconds.
config-unlimited-rate = Not limited
config-threads-high = { $threads } threads put heavy load on the server. Make sure the site can handle it.
