#!/usr/bin/env python3
# /// script
# requires-python = ">=3.8"
# dependencies = [
#     "graphql-core>=3.2.0",
# ]
# ///
#
# To run with uv (recommended): uv run check_schema.py
# To run without uv: python3 check_schema.py (requires graphql-core installed)
"""
GraphQL Schema Validation Tool

Compares the target GraphQL schema against the generated schema to identify
mismatches in types, fields, scalars, enums, unions, and subscriptions.

Uses proper GraphQL parsing with graphql-core for comprehensive AST-based validation.
"""

import sys
from typing import List, Tuple
from graphql import (
    build_schema,
    GraphQLSchema,
    GraphQLObjectType,
    GraphQLEnumType,
    GraphQLUnionType,
    GraphQLField,
    is_non_null_type,
    is_list_type,
)


def print_section(title: str):
    """Print a formatted section header"""
    print(f"\n{'=' * 80}")
    print(f"{title}")
    print("=" * 80)


def print_error(message: str):
    """Print an error message"""
    print(f"[ERROR] {message}")


def print_warning(message: str):
    """Print a warning message"""
    print(f"[WARN] {message}")


def print_success(message: str):
    """Print a success message"""
    print(f"[INFO] {message}")


def type_to_string(type_) -> str:
    """Convert a GraphQL type to its string representation"""
    if is_non_null_type(type_):
        return f"{type_to_string(type_.of_type)}!"
    elif is_list_type(type_):
        return f"[{type_to_string(type_.of_type)}]"
    else:
        return str(type_)


def compare_field_type(
    target_field: GraphQLField,
    actual_field: GraphQLField,
    field_name: str,
    type_name: str,
) -> List[str]:
    """Compare field types between target and actual schemas"""
    errors = []

    target_type_str = type_to_string(target_field.type)
    actual_type_str = type_to_string(actual_field.type)

    if target_type_str != actual_type_str:
        errors.append(
            f"{type_name}.{field_name}: Type mismatch\n"
            f"  Expected: {target_type_str}\n"
            f"  Got:      {actual_type_str}"
        )

    return errors


def compare_object_types(
    target: GraphQLObjectType, actual: GraphQLObjectType, type_name: str
) -> Tuple[List[str], List[str]]:
    """Compare two object types"""
    errors = []
    warnings = []

    target_fields = set(target.fields.keys())
    actual_fields = set(actual.fields.keys())

    # Check for missing fields
    missing_fields = target_fields - actual_fields
    if missing_fields:
        errors.append(f"{type_name}: Missing fields: {sorted(missing_fields)}")

    # Check for extra fields
    extra_fields = actual_fields - target_fields
    if extra_fields:
        warnings.append(f"{type_name}: Extra fields: {sorted(extra_fields)}")

    # Compare common fields
    for field_name in target_fields & actual_fields:
        target_field = target.fields[field_name]
        actual_field = actual.fields[field_name]
        field_errors = compare_field_type(
            target_field, actual_field, field_name, type_name
        )
        errors.extend(field_errors)

    return errors, warnings


def compare_enum_types(
    target: GraphQLEnumType, actual: GraphQLEnumType, type_name: str
) -> Tuple[List[str], List[str]]:
    """Compare two enum types"""
    errors = []
    warnings = []

    target_values = set(target.values.keys())
    actual_values = set(actual.values.keys())

    missing_values = target_values - actual_values
    if missing_values:
        errors.append(f"{type_name}: Missing enum values: {sorted(missing_values)}")

    extra_values = actual_values - target_values
    if extra_values:
        warnings.append(f"{type_name}: Extra enum values: {sorted(extra_values)}")

    return errors, warnings


def compare_union_types(
    target: GraphQLUnionType, actual: GraphQLUnionType, type_name: str
) -> Tuple[List[str], List[str]]:
    """Compare two union types"""
    errors = []
    warnings = []

    target_types = {str(t) for t in target.types}
    actual_types = {str(t) for t in actual.types}

    missing_types = target_types - actual_types
    if missing_types:
        errors.append(f"{type_name}: Missing union members: {sorted(missing_types)}")

    extra_types = actual_types - target_types
    if extra_types:
        warnings.append(f"{type_name}: Extra union members: {sorted(extra_types)}")

    return errors, warnings


def compare_schemas(
    target_schema: GraphQLSchema, actual_schema: GraphQLSchema
) -> Tuple[List[str], List[str]]:
    """Compare two GraphQL schemas comprehensively"""
    errors = []
    warnings = []

    # Get all type names (excluding built-in types)
    target_types = {
        name for name in target_schema.type_map.keys() if not name.startswith("__")
    }
    actual_types = {
        name for name in actual_schema.type_map.keys() if not name.startswith("__")
    }

    # Check for missing types
    missing_types = target_types - actual_types
    if missing_types:
        errors.append(f"Missing types: {sorted(missing_types)}")

    # Check for extra types
    extra_types = actual_types - target_types
    if extra_types:
        # Filter out common auto-generated types
        extra_types = {
            t for t in extra_types if not t.endswith("Error") or t in target_types
        }
        if extra_types:
            warnings.append(f"Extra types: {sorted(extra_types)}")

    # Compare common types
    for type_name in target_types & actual_types:
        target_type = target_schema.type_map[type_name]
        actual_type = actual_schema.type_map[type_name]

        # Ensure both are same kind of type
        if type(target_type) != type(actual_type):
            errors.append(
                f"{type_name}: Type kind mismatch\n"
                f"  Expected: {type(target_type).__name__}\n"
                f"  Got:      {type(actual_type).__name__}"
            )
            continue

        # Compare based on type kind
        if isinstance(target_type, GraphQLObjectType):
            type_errors, type_warnings = compare_object_types(
                target_type, actual_type, type_name
            )
            errors.extend(type_errors)
            warnings.extend(type_warnings)
        elif isinstance(target_type, GraphQLEnumType):
            type_errors, type_warnings = compare_enum_types(
                target_type, actual_type, type_name
            )
            errors.extend(type_errors)
            warnings.extend(type_warnings)
        elif isinstance(target_type, GraphQLUnionType):
            type_errors, type_warnings = compare_union_types(
                target_type, actual_type, type_name
            )
            errors.extend(type_errors)
            warnings.extend(type_warnings)
        # GraphQLScalarType, GraphQLInputObjectType, and GraphQLInterfaceType
        # are compared by name only (already validated above)

    return errors, warnings


def main():
    """Main validation function"""
    print(f"GraphQL Schema Validation")
    print(f"Comparing: design/target-api-schema.graphql vs schema.graphql\n")

    # Read schema files
    try:
        with open("design/target-api-schema.graphql", "r") as f:
            target_schema_str = f.read()
    except FileNotFoundError:
        print_error("Target schema file not found: design/target-api-schema.graphql")
        return 1

    try:
        with open("schema.graphql", "r") as f:
            actual_schema_str = f.read()
    except FileNotFoundError:
        print_error("Generated schema file not found: schema.graphql")
        print("Run: just export-schema-sqlite")
        return 1

    # Build schemas
    try:
        target_schema = build_schema(target_schema_str)
    except Exception as e:
        print_error(f"Failed to parse target schema: {e}")
        return 1

    try:
        actual_schema = build_schema(actual_schema_str)
    except Exception as e:
        print_error(f"Failed to parse actual schema: {e}")
        return 1

    # Compare schemas
    errors, warnings = compare_schemas(target_schema, actual_schema)

    # Print results
    print_section("VALIDATION RESULTS")

    if errors:
        print(f"\nCRITICAL ERRORS:")
        for error in errors:
            print_error(error)

    if warnings:
        print(f"\nWARNINGS:")
        for warning in warnings:
            print_warning(warning)

    if not errors and not warnings:
        print_success(
            "All schema validations passed! Target and actual schemas match perfectly."
        )

    # Summary
    print_section("SUMMARY")
    print(f"Errors:   {len(errors)}")
    print(f"Warnings: {len(warnings)}")

    # Exit code
    if errors:
        print(f"\nSCHEMA VALIDATION FAILED")
        return 1
    elif warnings:
        print(f"\n SCHEMA VALIDATION PASSED WITH WARNINGS")
        return 0
    else:
        print(f"\n SCHEMA VALIDATION PASSED")
        return 0


if __name__ == "__main__":
    sys.exit(main())
