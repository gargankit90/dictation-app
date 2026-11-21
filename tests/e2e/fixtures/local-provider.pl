#!/usr/bin/perl
# A stand-in for OpenRouter on the VM's loopback (T4a, Q2): it speaks the
# `POST …/audio/transcriptions` API the app's real engine uses (a JSON body
# with the WAV in base64; a multipart form is read too), so the signed test
# build (pointed here with --debug-provider-url) goes through its real HTTP
# client, keychain key, retries and JSON parsing, with no network and no
# real key. `POST …/chat/completions` (the cleanup pass) answers with the
# transcript it was sent, unchanged (or with `cleanup-text`, below), so
# cleanup is applied and the paste is what the speech reply said; it is logged
# as `cleanup`, not `request`, so the request numbers stay the speech
# requests'. It is held only while `cleanup-gate` exists (QT1).
#
#   perl local-provider.pl <state-dir> [port]      # default port 18731
#
# Plain Perl 5 with core modules only (the VM has no Python). Each connection
# is served in a process of its own, so a held reply doesn't block the next
# request. It is controlled, and read back, through files in <state-dir>:
#
#   accept-key.sha256  the SHA-256 (hex) of the one key it accepts; any other
#                      key, or none, gets a 401 as OpenRouter gives. Only the hash
#                      is on disk, so a test can check that no file holds the
#                      key itself.
#   gate               while this file exists, a reply is held after its
#                      request has been read whole ("the gate"); removing it
#                      releases every held reply.
#   cleanup-gate       like `gate`, for the cleanup (chat) reply only: it is
#                      held while this file exists, so the app's 1.5 s
#                      deadline passes and it pastes the raw transcript.
#   cleanup-text       what the cleanup answers instead of echoing its input
#                      (UTF-8, as is); read when the request arrives.
#   reply-text         the transcript to answer with (UTF-8, as is: leading
#                      and trailing whitespace and newlines are kept);
#                      "hello world" when absent. Read when the request
#                      arrives, so a held reply keeps the text it came with.
#   events             what happened, one ASCII line each, appended:
#                        listening port=<p> pid=<pid>
#                        request n=<n> path=<p> auth=good|bad|none model=<m>
#                          language=<l> format=<f> audio_bytes=<b>
#                        cleanup auth=good|bad|none status=<s>
#                        cleanup_held       the cleanup reply is held
#                        cleanup_closed     the client hung up while held
#                        held n=<n>
#                        closed n=<n>       the client hung up while held
#                        released n=<n> closed=0|1
#                        replied n=<n> status=<s>
#                      <n> counts requests from 1. Never a key or a
#                      transcript.
#   pid                the listener's pid
#   connect-key        the key `POST …/auth/keys` hands out (Q3, Connect
#                      OpenRouter); a made-up one, which accept-key.sha256
#                      should accept. Without the file the exchange fails (500).
#   force-status       a status code (402, ...): every speech and cleanup
#                      request with the good key gets that status, with
#                      OpenRouter's error body, instead of its answer.
#
# Connect OpenRouter (Q3), as the app's test build (--debug-provider-url) drives
# it: `GET …/auth?callback_url=…&code_challenge=…&state=…` plays OpenRouter's
# sign-in page and the user who agrees: it calls the callback back at once
# with a code and the same state. `POST …/auth/keys` checks the code and the
# PKCE pair (S256 of the verifier must be the challenge) and answers
# {"key": <connect-key>}. Events: `auth-start`, `auth-callback status=<s>`,
# `exchange pkce=ok|bad code=ok|bad`; never the key, code or verifier.
#
# GET /health answers 200 "ok" and isn't counted.
use strict;
use warnings;
use Digest::SHA qw(sha256_hex);
use Fcntl qw(:flock O_WRONLY O_APPEND O_CREAT);
use IO::Select;
use IO::Socket::INET;
use JSON::PP;
use MIME::Base64 ();

my ($dir, $port) = @ARGV;
die "usage: local-provider.pl <state-dir> [port]\n" unless defined $dir && -d $dir;
$port ||= 18731;
# The longest a reply is held, so a test that forgets the gate can't leave a
# process behind for ever.
my $MAX_HOLD = 120;
my $DEFAULT_TEXT = "hello world";

$SIG{PIPE} = 'IGNORE';
$SIG{CHLD} = 'IGNORE';

my $server = IO::Socket::INET->new(
  LocalAddr => '127.0.0.1',
  LocalPort => $port,
  Listen    => 16,
  ReuseAddr => 1,
  Proto     => 'tcp',
) or die "local-provider: can't listen on 127.0.0.1:$port: $!\n";

write_file("$dir/pid", "$$\n");
event("listening port=$port pid=$$");

while (1) {
  my $client = $server->accept or next;
  my $pid = fork;
  if (!defined $pid) {
    close $client;
    next;
  }
  if ($pid == 0) {
    close $server;
    serve($client);
    exit 0;
  }
  close $client;
}

sub write_file {
  my ($path, $data) = @_;
  open my $fh, '>', "$path.tmp" or die "can't write $path: $!";
  print $fh $data;
  close $fh;
  rename "$path.tmp", $path or die "can't rename $path: $!";
}

# Appends one line to the events file under a lock.
sub event {
  my ($line) = @_;
  sysopen my $fh, "$dir/events", O_WRONLY | O_APPEND | O_CREAT or die "events: $!";
  flock $fh, LOCK_EX;
  syswrite $fh, "$line\n";
  close $fh;
}

# The next request number, taken and logged in one step under the lock.
sub log_request {
  my ($rest) = @_;
  sysopen my $fh, "$dir/events", O_WRONLY | O_APPEND | O_CREAT or die "events: $!";
  flock $fh, LOCK_EX;
  my $n = 1;
  if (open my $in, '<', "$dir/events") {
    while (<$in>) { $n++ if /^request /; }
    close $in;
  }
  syswrite $fh, "request n=$n $rest\n";
  close $fh;
  return $n;
}

# Reads one request: (method, path, lower-cased headers, body), or () if the
# client hung up first.
sub read_request {
  my ($sock) = @_;
  my $data = '';
  my $end;
  while (1) {
    my $got = sysread $sock, my $chunk, 65536;
    return () unless $got;
    $data .= $chunk;
    $end = index $data, "\r\n\r\n";
    last if $end >= 0;
  }
  my $head = substr $data, 0, $end;
  my $body = substr $data, $end + 4;
  my ($request_line, @lines) = split /\r\n/, $head;
  my ($method, $path) = split / /, $request_line;
  my %headers;
  for (@lines) {
    my ($name, $value) = /^([^:]+):\s*(.*)$/ or next;
    $headers{lc $name} = $value;
  }
  if (($headers{'transfer-encoding'} // '') =~ /chunked/i) {
    my $decoded = '';
    while (1) {
      while ($body !~ /\r\n/) {
        my $got = sysread $sock, my $chunk, 65536;
        return () unless $got;
        $body .= $chunk;
      }
      my ($size_hex) = $body =~ /^([0-9a-fA-F]+)/;
      my $size = hex($size_hex // '0');
      my $start = index($body, "\r\n") + 2;
      while (length($body) < $start + $size + 2) {
        my $got = sysread $sock, my $chunk, 65536;
        return () unless $got;
        $body .= $chunk;
      }
      last if $size == 0;
      $decoded .= substr $body, $start, $size;
      $body = substr $body, $start + $size + 2;
    }
    $body = $decoded;
  } else {
    my $length = $headers{'content-length'} // 0;
    while (length($body) < $length) {
      my $got = sysread $sock, my $chunk, 65536;
      return () unless $got;
      $body .= $chunk;
    }
  }
  return ($method, $path, \%headers, $body);
}

# The JSON body as a hash, or undef if it isn't a JSON object.
sub json_body {
  my ($body) = @_;
  my $value = eval { JSON::PP->new->utf8->decode($body) };
  return ref $value eq 'HASH' ? $value : undef;
}

# A request field: from the JSON body, else the multipart form; "-" if absent.
sub field {
  my ($json, $body, $name) = @_;
  if ($json) {
    my $value = $json->{$name};
    return defined $value && !ref $value ? $value : '-';
  }
  return form_field($body, $name);
}

# The audio's size in bytes: the base64 `input_audio.data` decoded, or the
# multipart file.
sub audio_bytes {
  my ($json, $body) = @_;
  if ($json) {
    require MIME::Base64;
    my $data = ref $json->{input_audio} eq 'HASH' ? $json->{input_audio}{data} : undef;
    return defined $data ? length MIME::Base64::decode_base64($data) : 0;
  }
  return form_file_bytes($body);
}

# The transcript between the tags of a chat request's user message.
sub cleanup_input {
  my ($json) = @_;
  my $messages = $json && ref $json->{messages} eq 'ARRAY' ? $json->{messages} : [];
  for my $message (@$messages) {
    next unless ref $message eq 'HASH' && ($message->{role} // '') eq 'user';
    my $content = $message->{content} // '';
    return $1 if $content =~ m{<transcript>\n(.*)\n</transcript>}s;
    return $content;
  }
  return '';
}

# A text field of a multipart form, or "-".
sub form_field {
  my ($body, $name) = @_;
  return $1 if $body =~ /name="\Q$name\E"\r\n(?:[^\r\n]+\r\n)*\r\n(.*?)\r\n--/s;
  return '-';
}

sub form_file_bytes {
  my ($body) = @_;
  return length $1 if $body =~ /name="file"[^\r\n]*\r\n(?:[^\r\n]+\r\n)*\r\n(.*?)\r\n--/s;
  return 0;
}

sub respond {
  my ($sock, $status, $reason, $body) = @_;
  my $response = "HTTP/1.1 $status $reason\r\n"
    . "content-type: application/json\r\n"
    . "content-length: " . length($body) . "\r\n"
    . "connection: close\r\n\r\n$body";
  syswrite $sock, $response;
}

# Holds the reply while the gate file exists; says when the client hangs
# up. Returns whether it did.
sub hold {
  my ($sock, $n, $gate, $label) = @_;
  $gate //= 'gate';
  return 0 unless -e "$dir/$gate";
  event($label ? "${label}_held" : "held n=$n");
  my $select = IO::Select->new($sock);
  my $closed = 0;
  my $until = time + $MAX_HOLD;
  while (-e "$dir/$gate" && time < $until) {
    if (!$closed && $select->can_read(0.05)) {
      my $got = sysread $sock, my $chunk, 4096;
      if (!$got) {
        $closed = 1;
        event($label ? "${label}_closed" : "closed n=$n");
      }
    } elsif ($closed) {
      select undef, undef, undef, 0.05;
    }
  }
  event("released n=$n closed=$closed") unless $label;
  return $closed;
}

sub reply_text {
  my ($name, $default) = @_;
  my $path = "$dir/" . ($name // 'reply-text');
  $default //= $DEFAULT_TEXT;
  return $default unless -e $path;
  open my $fh, '<:raw', $path or return $default;
  local $/;
  my $bytes = <$fh>;
  close $fh;
  return Encode::decode('UTF-8', $bytes // '');
}

sub serve {
  my ($sock) = @_;
  require Encode;
  my ($method, $path, $headers, $body) = read_request($sock);
  return unless defined $method;
  if ($method eq 'GET' && $path eq '/health') {
    respond($sock, 200, 'OK', '"ok"');
    return;
  }
  if ($method eq 'GET' && $path =~ m{/auth\?}) {
    sign_in($sock, $path);
    return;
  }
  if ($method eq 'POST' && $path =~ m{/auth/keys$}) {
    exchange($sock, $body);
    return;
  }
  my $auth = 'none';
  if (($headers->{authorization} // '') =~ /^Bearer (\S+)$/) {
    my $want = '';
    if (open my $fh, '<', "$dir/accept-key.sha256") {
      $want = <$fh> // '';
      close $fh;
      $want =~ s/\s+//g;
    }
    $auth = ($want ne '' && sha256_hex($1) eq lc $want) ? 'good' : 'bad';
  }
  my $json = json_body($body);
  if ($method eq 'POST' && $path =~ m{/chat/completions$}) {
    my ($status, $reply) = (401,
      '{"error":{"message":"Invalid API Key","code":401}}');
    if ($auth eq 'good' && forced_status()) {
      $status = forced_status();
      $reply = '{"error":{"message":"Insufficient credits","code":' . $status . '}}';
    } elsif ($auth eq 'good') {
      my $content = reply_text('cleanup-text', cleanup_input($json));
      hold($sock, 0, 'cleanup-gate', 'cleanup');
      $reply = JSON::PP->new->ascii->canonical->encode({
        choices => [{ message => { role => 'assistant', content => $content }, finish_reason => 'stop' }],
      });
      $status = 200;
    }
    respond($sock, $status, $status == 200 ? 'OK' : 'Error', $reply);
    event("cleanup auth=$auth status=$status");
    return;
  }
  my $fields = sprintf 'model=%s language=%s format=%s audio_bytes=%d',
    field($json, $body, 'model'), field($json, $body, 'language'),
    field($json, $body, 'response_format'), audio_bytes($json, $body);
  $fields =~ s/[^\x20-\x7e]/?/g;
  (my $safe_path = $path) =~ s/[^\x21-\x7e]/?/g;
  my $text = reply_text();
  my $n = log_request("path=$safe_path auth=$auth $fields");
  hold($sock, $n);
  my ($status, $reason, $reply);
  if ($method ne 'POST' || $path !~ m{/audio/transcriptions$}) {
    ($status, $reason, $reply) = (404, 'Not Found', '{"error":{"message":"not found"}}');
  } elsif ($auth eq 'good' && forced_status()) {
    $status = forced_status();
    ($status, $reason, $reply) = ($status, 'Error',
      '{"error":{"message":"Insufficient credits","code":' . $status . '}}');
  } elsif ($auth ne 'good') {
    ($status, $reason, $reply) = (401, 'Unauthorized',
      '{"error":{"message":"Invalid API Key","type":"invalid_request_error","code":"invalid_api_key"}}');
  } else {
    # ASCII JSON: every non-ASCII character as a \u escape (a non-BMP one as
    # a surrogate pair), which the app's JSON parser must put back together.
    $reply = JSON::PP->new->ascii->canonical->encode({
      text     => $text,
      language => 'english',
      duration => 1.5,
      segments => [{ id => 0, start => 0, end => 1.5, text => $text, no_speech_prob => 0.01 }],
    });
    ($status, $reason) = (200, 'OK');
  }
  respond($sock, $status, $reason, $reply);
  event("replied n=$n status=$status");
}

# The status in the force-status file, or 0.
sub forced_status {
  open my $fh, '<', "$dir/force-status" or return 0;
  my $code = <$fh> // '';
  close $fh;
  return $code =~ /^\s*(\d{3})\s*$/ ? $1 : 0;
}

sub url_decode {
  my ($v) = @_;
  $v =~ tr/+/ /;
  $v =~ s/%([0-9a-fA-F]{2})/chr hex $1/ge;
  return $v;
}

# OpenRouter's sign-in page, with the user agreeing: calls the callback back.
sub sign_in {
  my ($sock, $path) = @_;
  my ($query) = $path =~ /\?(.*)$/;
  my %q = map { my ($k, $v) = split /=/, $_, 2; (url_decode($k), url_decode($v // '')) } split /&/, $query;
  event("auth-start");
  write_file("$dir/challenge", ($q{code_challenge} // '') . "\n");
  respond($sock, 200, 'OK', '"signed in"');
  close $sock;
  my $callback = $q{callback_url} // '';
  my ($port, $cb_path) = $callback =~ m{^http://(?:localhost|127\.0\.0\.1):(\d+)(/[^?]*)$} or return;
  my $conn = IO::Socket::INET->new(PeerAddr => '127.0.0.1', PeerPort => $port, Proto => 'tcp', Timeout => 5) or return;
  my $state = $q{state} // '';
  syswrite $conn, "GET $cb_path?code=e2efakecode&state=$state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
  my $answer = '';
  while (sysread $conn, my $chunk, 4096) { $answer .= $chunk; }
  my ($status) = $answer =~ m{^HTTP/1\.[01] (\d+)};
  event("auth-callback status=" . ($status // 0));
}

# The code-for-key exchange.
sub exchange {
  my ($sock, $body) = @_;
  my $json = json_body($body) || {};
  my $want = '';
  if (open my $fh, '<', "$dir/challenge") { $want = <$fh> // ''; close $fh; $want =~ s/\s+//g; }
  my $verifier = $json->{code_verifier} // '';
  my $made = MIME::Base64::encode_base64url(Digest::SHA::sha256($verifier));
  my $pkce = ($want ne '' && $made eq $want && ($json->{code_challenge_method} // '') eq 'S256') ? 'ok' : 'bad';
  my $code = ($json->{code} // '') eq 'e2efakecode' ? 'ok' : 'bad';
  event("exchange pkce=$pkce code=$code");
  my $key = '';
  if (open my $fh, '<', "$dir/connect-key") { $key = <$fh> // ''; close $fh; $key =~ s/\s+//g; }
  if ($pkce ne 'ok' || $code ne 'ok') {
    respond($sock, 400, 'Bad Request', '{"error":{"message":"bad code","code":400}}');
  } elsif ($key eq '') {
    respond($sock, 500, 'Error', '{"error":{"message":"no key","code":500}}');
  } else {
    respond($sock, 200, 'OK', JSON::PP->new->canonical->encode({ key => $key, user_id => 'e2e' }));
  }
}
