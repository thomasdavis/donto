# PGXS-style build glue. The actual extension SQL is concatenated from
# sql/migrations into pg_donto--<version>.sql at install time.
#
# Build: make
# Install: sudo make install
# Then in psql: CREATE EXTENSION pg_donto;

EXTENSION = pg_donto
DATA = pg_donto--0.1.0.sql
PG_CONFIG = pg_config

PG_CPPFLAGS = -I.

include $(shell $(PG_CONFIG) --pgxs)/contrib/contrib-global.mk

# Build the install script by concatenating migrations.
pg_donto--0.1.0.sql: ../sql/migrations/*.sql
	cat ../sql/migrations/0001_core.sql \
	    ../sql/migrations/0002_flags.sql \
	    ../sql/migrations/0003_functions.sql \
	    ../sql/migrations/0004_migrations.sql \
	    > $@
