
# smtpserver

new emails not stored untill new connection arrives
 - need to monitor for new connections and thread termination at the same time

# smtprelay

because of Send + Sync requirements on check_user and check_pass closures it no longer compiles
 - simply cache the users and passwords on a Sync friendly vec before hand
 - then no database queries are required.
 - hence Send + Sync safety :thumbsup:
