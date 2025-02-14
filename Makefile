prefix ?= /usr
sysconfdir ?= /etc
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

SRC = Cargo.toml Cargo.lock Makefile $(shell find src -type f -wholename '*src/*.rs')

.PHONY: all clean distclean install uninstall update

BIN=system76-power
RDD=com.system76.PowerDaemon
POLICY=com.system76.powerdaemon

DEBUG ?= 0
ifeq ($(DEBUG),0)
	ARGS += "--release"
	TARGET = release
endif

VENDORED ?= 0
ifeq ($(VENDORED),1)
	ARGS += "--frozen"
endif

all: target/release/$(BIN)

clean:
	cargo clean

distclean:
	rm -rf .cargo vendor vendor.tar.xz

install: all
	install -D -m 0755 "target/release/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -D -m 0644 "data/$(RDD).xml" "$(DESTDIR)$(sysconfdir)/dbus-1/interfaces/$(RDD).xml"
	install -D -m 0644 "data/$(BIN).conf" "$(DESTDIR)$(sysconfdir)/dbus-1/system.d/$(BIN).conf"
	install -D -m 0644 "debian/$(BIN).service" "$(DESTDIR)$(sysconfdir)/systemd/system/$(BIN).service"
	install -D -m 0644 "data/$(POLICY).policy" $(DESTDIR)$(datadir)/polkit-1/actions/$(POLICY).policy

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(sysconfdir)/dbus-1/system.d/$(BIN).conf"
	rm -f "$(DESTDIR)$(sysconfdir)/systemd/system/$(BIN).service"
	rm -f "$(DESTDIR)$(sysconfdir)/dbus-1/interfaces/$(RDD).xml"

update:
	cargo update

vendor:
	mkdir -p .cargo
	cargo vendor | head -n -1 > .cargo/config
	echo 'directory = "vendor"' >> .cargo/config
	tar pcfJ vendor.tar.xz vendor
	rm -rf vendor

target/release/$(BIN): $(SRC)
ifeq ($(VENDORED),1)
	tar pxf vendor.tar.xz
endif
	cargo build $(ARGS)
