import re

target = open('design/target-api-schema.graphql').read()
actual = open('schema.graphql').read()

# Extract scalar definitions
scalars_target = set(re.findall(r'^scalar\s+(\w+)', target, re.MULTILINE))
scalars_actual = set(re.findall(r'^scalar\s+(\w+)', actual, re.MULTILINE))

print("=" * 80)
print("SCALAR MISMATCHES")
print("=" * 80)
print(f"Target scalars: {sorted(scalars_target)}")
print(f"Actual scalars: {sorted(scalars_actual)}")

missing_scalars = scalars_target - scalars_actual
extra_scalars = scalars_actual - scalars_target

if missing_scalars:
    print(f"❌ Missing scalars: {sorted(missing_scalars)}")
if extra_scalars:
    print(f"⚠️  Extra scalars: {sorted(extra_scalars)}")

# Check field types in key types
print("\n" + "=" * 80)
print("CRITICAL FIELD TYPE MISMATCHES")
print("=" * 80)

# Extract ChainInfo.ticketPrice, keyBindingFee types
for pattern in [
    (r'type ChainInfo.*?ticketPrice:\s*(\w+)'),
    (r'type ChainInfo.*?keyBindingFee:\s*(\w+)'),
    (r'type ChainInfo.*?channelClosureGracePeriod:\s*(\w+)'),
    (r'type Channel.*?balance:\s*(\w+)'),
    (r'type Channel.*?ticketIndex:\s*(\w+)'),
]:
    target_match = re.search(pattern, target, re.DOTALL)
    actual_match = re.search(pattern, actual, re.DOTALL)
    
    field_name = pattern.split('.*?')[-1].split(':')[0] if '.*?' in pattern else pattern
    target_val = target_match.group(1) if target_match else 'NOT FOUND'
    actual_val = actual_match.group(1) if actual_match else 'NOT FOUND'
    
    if target_val != actual_val:
        print(f"❌ {field_name}: expected '{target_val}', got '{actual_val}'")

# Check for extra subscriptions
print("\n" + "=" * 80)
print("SUBSCRIPTION DIFFERENCES")
print("=" * 80)

target_subs = set(re.findall(r'^\s+(\w+)\(', re.search(r'type SubscriptionRoot \{(.*?)\n\}', target, re.DOTALL).group(1), re.MULTILINE))
actual_subs = set(re.findall(r'^\s+(\w+)\(', re.search(r'type SubscriptionRoot \{(.*?)\n\}', actual, re.DOTALL).group(1), re.MULTILINE))

print(f"Target subscriptions: {sorted(target_subs)}")
print(f"Actual subscriptions: {sorted(actual_subs)}")

missing_subs = target_subs - actual_subs
extra_subs = actual_subs - target_subs

if missing_subs:
    print(f"❌ Missing subscriptions: {sorted(missing_subs)}")
if extra_subs:
    print(f"⚠️  Extra subscriptions: {sorted(extra_subs)}")

# Check for Transaction.id type
print("\n" + "=" * 80)
print("TRANSACTION ID TYPE")
print("=" * 80)

target_tx_id = re.search(r'type Transaction.*?id:\s*(\w+!)?\s', target, re.DOTALL)
actual_tx_id = re.search(r'type Transaction.*?id:\s*(\w+)!?', actual, re.DOTALL)

target_id_type = target_tx_id.group(1) if target_tx_id else "NOT FOUND"
actual_id_type = actual_tx_id.group(1) if actual_tx_id else "NOT FOUND"

print(f"Target: Transaction.id type = {target_id_type}")
print(f"Actual: Transaction.id type = {actual_id_type}")
if target_id_type != actual_id_type:
    print(f"❌ TYPE MISMATCH")
