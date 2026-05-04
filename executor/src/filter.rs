use crate::row_format::RowView;
use crate::typed_value::TypedValue;
use chrono::Utc;
use schema::ColumnInfo;
use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    TrimWhereField, UnaryOperator, Value,
};
use std::collections::HashMap;

/// A deserialized row: col_name -> TypedValue.
pub type Row = HashMap<String, TypedValue>;

/// Evaluate a WHERE expression against a row. Returns true if the row matches.
pub fn evaluate_predicate(expr: &Expr, row: &Row, schema_cols: &[ColumnInfo]) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => eval_binary_op(left, op, right, row, schema_cols),
        Expr::UnaryOp { op: UnaryOperator::Not, expr } => {
            !evaluate_predicate(expr, row, schema_cols)
        }
        Expr::IsNull(inner) => {
            resolve_value(inner, row, schema_cols).is_null()
        }
        Expr::IsNotNull(inner) => {
            !resolve_value(inner, row, schema_cols).is_null()
        }
        Expr::Between { expr: col, low, high, negated } => {
            let val = resolve_value(col, row, schema_cols);
            let lo = resolve_value(low, row, schema_cols);
            let hi = resolve_value(high, row, schema_cols);
            let in_range = (val.sql_eq(&lo) || val.sql_gt(&lo))
                && (val.sql_eq(&hi) || val.sql_lt(&hi));
            if *negated { !in_range } else { in_range }
        }
        Expr::InList { expr: col, list, negated } => {
            let val = resolve_value(col, row, schema_cols);
            let found = list.iter().any(|item| {
                let item_val = resolve_value(item, row, schema_cols);
                val.sql_eq(&item_val)
            });
            if *negated { !found } else { found }
        }
        Expr::Like { expr: col_expr, pattern, negated, .. } => {
            let val = resolve_value(col_expr, row, schema_cols);
            let pat = resolve_value(pattern, row, schema_cols);
            let matches = if let TypedValue::Text(p) = &pat {
                val.sql_like(p)
            } else {
                false
            };
            if *negated { !matches } else { matches }
        }
        Expr::Nested(inner) => evaluate_predicate(inner, row, schema_cols),
        _ => false,
    }
}

fn eval_binary_op(
    left: &Expr,
    op: &BinaryOperator,
    right: &Expr,
    row: &Row,
    schema_cols: &[ColumnInfo],
) -> bool {
    // Handle AND/OR first (short-circuit)
    match op {
        BinaryOperator::And => {
            evaluate_predicate(left, row, schema_cols)
                && evaluate_predicate(right, row, schema_cols)
        }
        BinaryOperator::Or => {
            evaluate_predicate(left, row, schema_cols)
                || evaluate_predicate(right, row, schema_cols)
        }
        _ => {
            let lval = resolve_value(left, row, schema_cols);
            let rval = resolve_value(right, row, schema_cols);
            match op {
                BinaryOperator::Eq => lval.sql_eq(&rval),
                BinaryOperator::NotEq => !lval.sql_eq(&rval) && !lval.is_null() && !rval.is_null(),
                BinaryOperator::Lt => lval.sql_lt(&rval),
                BinaryOperator::LtEq => lval.sql_lt(&rval) || lval.sql_eq(&rval),
                BinaryOperator::Gt => lval.sql_gt(&rval),
                BinaryOperator::GtEq => lval.sql_gt(&rval) || lval.sql_eq(&rval),
                _ => false,
            }
        }
    }
}

/// Resolve an AST expression to a TypedValue using the row's data.
pub fn resolve_value(expr: &Expr, row: &Row, schema_cols: &[ColumnInfo]) -> TypedValue {
    match expr {
        Expr::Identifier(ident) => {
            let col_name = ident.value.to_lowercase();
            // Try exact match first, then search for any key ending with .col_name (for JOIN rows)
            row.get(&col_name)
                .or_else(|| row.iter().find(|(k, _)| k.ends_with(&format!(".{}", col_name))).map(|(_, v)| v))
                .cloned()
                .unwrap_or(TypedValue::Null)
        }
        Expr::CompoundIdentifier(parts) => {
            // Try fully qualified alias.column first (for JOIN rows)
            let qualified = parts.iter().map(|p| p.value.to_lowercase()).collect::<Vec<_>>().join(".");
            if let Some(val) = row.get(&qualified) {
                return val.clone();
            }
            // Fall back to last part only
            let col_name = parts.last().map(|p| p.value.to_lowercase()).unwrap_or_default();
            row.get(&col_name).cloned().unwrap_or(TypedValue::Null)
        }
        Expr::Value(val) => literal_to_typed(val),
        Expr::TypedString { value, .. } => TypedValue::Text(value.clone()),
        Expr::UnaryOp { op: UnaryOperator::Minus, expr } => {
            if let TypedValue::Int(i) = resolve_value(expr, row, schema_cols) {
                TypedValue::Int(-i)
            } else if let TypedValue::Float(f) = resolve_value(expr, row, schema_cols) {
                TypedValue::Float(-f)
            } else {
                TypedValue::Null
            }
        }
        Expr::BinaryOp { left, op, right } => {
            eval_arithmetic(left, op, right, row, schema_cols)
        }
        Expr::Nested(inner) => resolve_value(inner, row, schema_cols),
        Expr::Case { operand, conditions, results, else_result, .. } => {
            eval_case(operand.as_deref(), conditions, results, else_result.as_deref(), row, schema_cols)
        }
        Expr::Function(f) => {
            let mut resolver = |inner: &Expr| resolve_value(inner, row, schema_cols);
            eval_function(f, &mut resolver)
        }
        Expr::Cast { expr, data_type, .. } => {
            let val = resolve_value(expr, row, schema_cols);
            val.cast_to_data_type(data_type).unwrap_or(TypedValue::Null)
        }
        Expr::Substring { expr, substring_from, substring_for, .. } => {
            let mut resolver = |inner: &Expr| resolve_value(inner, row, schema_cols);
            eval_substring_expr(expr, substring_from.as_deref(), substring_for.as_deref(), &mut resolver)
        }
        Expr::Trim { expr, trim_where, trim_what, trim_characters } => {
            let mut resolver = |inner: &Expr| resolve_value(inner, row, schema_cols);
            eval_trim_expr(
                expr,
                trim_where.as_ref(),
                trim_what.as_deref(),
                trim_characters.as_deref(),
                &mut resolver,
            )
        }
        _ => TypedValue::Null,
    }
}

/// Evaluate arithmetic binary operations (+, -, *, /).
fn eval_arithmetic(
    left: &Expr,
    op: &BinaryOperator,
    right: &Expr,
    row: &Row,
    schema_cols: &[ColumnInfo],
) -> TypedValue {
    let lval = resolve_value(left, row, schema_cols);
    let rval = resolve_value(right, row, schema_cols);

    if lval.is_null() || rval.is_null() {
        return TypedValue::Null;
    }

    let lf = lval.as_f64();
    let rf = rval.as_f64();

    match (lf, rf) {
        (Some(l), Some(r)) => {
            let result = match op {
                BinaryOperator::Plus => l + r,
                BinaryOperator::Minus => l - r,
                BinaryOperator::Multiply => l * r,
                BinaryOperator::Divide => {
                    if r == 0.0 {
                        return TypedValue::Null;
                    }
                    l / r
                }
                BinaryOperator::Modulo => {
                    if r == 0.0 {
                        return TypedValue::Null;
                    }
                    l % r
                }
                _ => return TypedValue::Null,
            };
            // Preserve integer type when both inputs are integers and result is whole
            if matches!(lval, TypedValue::Int(_))
                && matches!(rval, TypedValue::Int(_))
                && result == (result as i64) as f64
                && !matches!(op, BinaryOperator::Divide)
            {
                TypedValue::Int(result as i64)
            } else {
                TypedValue::Float(result)
            }
        }
        _ => TypedValue::Null,
    }
}

/// Evaluate a CASE WHEN expression.
fn eval_case(
    operand: Option<&Expr>,
    conditions: &[Expr],
    results: &[Expr],
    else_result: Option<&Expr>,
    row: &Row,
    schema_cols: &[ColumnInfo],
) -> TypedValue {
    if let Some(op) = operand {
        // Simple CASE: CASE expr WHEN val1 THEN res1 ...
        let op_val = resolve_value(op, row, schema_cols);
        for (cond, res) in conditions.iter().zip(results.iter()) {
            let cond_val = resolve_value(cond, row, schema_cols);
            if op_val.sql_eq(&cond_val) {
                return resolve_value(res, row, schema_cols);
            }
        }
    } else {
        // Searched CASE: CASE WHEN cond1 THEN res1 ...
        for (cond, res) in conditions.iter().zip(results.iter()) {
            if evaluate_predicate(cond, row, schema_cols) {
                return resolve_value(res, row, schema_cols);
            }
        }
    }
    // ELSE
    else_result
        .map(|e| resolve_value(e, row, schema_cols))
        .unwrap_or(TypedValue::Null)
}

/// Convert a SQL literal value to TypedValue.
fn literal_to_typed(val: &Value) -> TypedValue {
    match val {
        Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                TypedValue::Int(i)
            } else if let Ok(f) = n.parse::<f64>() {
                TypedValue::Float(f)
            } else {
                TypedValue::Text(n.clone())
            }
        }
        Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) => {
            TypedValue::Text(s.clone())
        }
        Value::Boolean(b) => TypedValue::Boolean(*b),
        Value::Null => TypedValue::Null,
        _ => TypedValue::Null,
    }
}

fn eval_function<F>(f: &Function, resolve_expr: &mut F) -> TypedValue
where
    F: FnMut(&Expr) -> TypedValue,
{
    let name = f.name.to_string().to_uppercase();
    let args = match resolve_function_args(&f.args, resolve_expr) {
        Some(args) => args,
        None => return TypedValue::Null,
    };

    match name.as_str() {
        "COALESCE" => args
            .into_iter()
            .find(|value| !value.is_null())
            .unwrap_or(TypedValue::Null),
        "NULLIF" => {
            if args.len() != 2 {
                return TypedValue::Null;
            }
            if args[0].sql_eq(&args[1]) {
                TypedValue::Null
            } else {
                args[0].clone()
            }
        }
        "LOWER" => unary_text_function(&args, |value| value.to_lowercase()),
        "UPPER" => unary_text_function(&args, |value| value.to_uppercase()),
        "LENGTH" => {
            if args.len() != 1 {
                return TypedValue::Null;
            }
            match &args[0] {
                TypedValue::Null => TypedValue::Null,
                TypedValue::Text(text) => TypedValue::Int(text.chars().count() as i64),
                _ => TypedValue::Null,
            }
        }
        "CONCAT" => {
            let mut out = String::new();
            for arg in args {
                if !arg.is_null() {
                    out.push_str(&arg.to_string());
                }
            }
            TypedValue::Text(out)
        }
        "NOW" | "CURRENT_TIMESTAMP" | "LOCALTIMESTAMP" => {
            if !args.is_empty() {
                return TypedValue::Null;
            }
            TypedValue::Text(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
        }
        "CURRENT_DATE" => {
            if !args.is_empty() {
                return TypedValue::Null;
            }
            TypedValue::Text(Utc::now().date_naive().format("%Y-%m-%d").to_string())
        }
        "CURRENT_TIME" | "LOCALTIME" => {
            if !args.is_empty() {
                return TypedValue::Null;
            }
            TypedValue::Text(Utc::now().format("%H:%M:%S").to_string())
        }
        _ => TypedValue::Null,
    }
}

fn resolve_function_args<F>(args: &FunctionArguments, resolve_expr: &mut F) -> Option<Vec<TypedValue>>
where
    F: FnMut(&Expr) -> TypedValue,
{
    match args {
        FunctionArguments::None => Some(Vec::new()),
        FunctionArguments::List(list) => {
            let mut values = Vec::with_capacity(list.args.len());
            for arg in &list.args {
                match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => {
                        values.push(resolve_expr(expr));
                    }
                    _ => return None,
                }
            }
            Some(values)
        }
        FunctionArguments::Subquery(_) => None,
    }
}

fn unary_text_function(args: &[TypedValue], mapper: impl FnOnce(&str) -> String) -> TypedValue {
    if args.len() != 1 {
        return TypedValue::Null;
    }

    match &args[0] {
        TypedValue::Null => TypedValue::Null,
        TypedValue::Text(text) => TypedValue::Text(mapper(text)),
        _ => TypedValue::Null,
    }
}

fn eval_substring_expr<F>(
    expr: &Expr,
    substring_from: Option<&Expr>,
    substring_for: Option<&Expr>,
    resolve_expr: &mut F,
) -> TypedValue
where
    F: FnMut(&Expr) -> TypedValue,
{
    let value = resolve_expr(expr);
    let text = match value {
        TypedValue::Null => return TypedValue::Null,
        TypedValue::Text(text) => text,
        _ => return TypedValue::Null,
    };

    let start = substring_from
        .map(|expr| resolve_expr(expr))
        .and_then(|value| value.as_i64())
        .unwrap_or(1)
        .max(1) as usize;
    let length = substring_for
        .map(|expr| resolve_expr(expr))
        .and_then(|value| value.as_i64())
        .map(|value| value.max(0) as usize);

    let chars: Vec<char> = text.chars().collect();
    let begin = start.saturating_sub(1);
    if begin >= chars.len() {
        return TypedValue::Text(String::new());
    }

    let end = length
        .map(|len| begin.saturating_add(len).min(chars.len()))
        .unwrap_or(chars.len());

    TypedValue::Text(chars[begin..end].iter().collect())
}

fn eval_trim_expr<F>(
    expr: &Expr,
    trim_where: Option<&TrimWhereField>,
    trim_what: Option<&Expr>,
    trim_characters: Option<&[Expr]>,
    resolve_expr: &mut F,
) -> TypedValue
where
    F: FnMut(&Expr) -> TypedValue,
{
    let value = resolve_expr(expr);
    let text = match value {
        TypedValue::Null => return TypedValue::Null,
        TypedValue::Text(text) => text,
        _ => return TypedValue::Null,
    };

    let trim_chars = if let Some(chars) = trim_characters {
        chars.first()
            .map(resolve_expr)
            .and_then(|value| match value {
                TypedValue::Text(text) => Some(text),
                _ => None,
            })
            .unwrap_or_else(|| " ".to_string())
    } else if let Some(trim_expr) = trim_what {
        match resolve_expr(trim_expr) {
            TypedValue::Text(text) => text,
            TypedValue::Null => return TypedValue::Null,
            _ => return TypedValue::Null,
        }
    } else {
        " ".to_string()
    };

    let char_set: Vec<char> = trim_chars.chars().collect();
    let trimmed = match trim_where.unwrap_or(&TrimWhereField::Both) {
        TrimWhereField::Both => text.trim_matches(|c| char_set.contains(&c)).to_string(),
        TrimWhereField::Leading => text.trim_start_matches(|c| char_set.contains(&c)).to_string(),
        TrimWhereField::Trailing => text.trim_end_matches(|c| char_set.contains(&c)).to_string(),
    };

    TypedValue::Text(trimmed)
}

/// Deserialize raw bytes into a Row (owned version).
pub fn bytes_to_row(col_values: &[(&ColumnInfo, Option<Vec<u8>>)]) -> Row {
    let mut row = Row::new();
    for (col, val) in col_values {
        let typed = match val {
            Some(bytes) => TypedValue::from_bytes(bytes, &col.data_type),
            None => TypedValue::Null,
        };
        row.insert(col.name.to_lowercase(), typed);
    }
    row
}

/// Zero-copy variant: takes borrowed byte slices, avoiding intermediate Vec allocations.
pub fn bytes_to_row_ref(col_values: &[(&ColumnInfo, Option<&[u8]>)]) -> Row {
    let mut row = Row::new();
    for (col, val) in col_values {
        let typed = match val {
            Some(bytes) => TypedValue::from_bytes(bytes, &col.data_type),
            None => TypedValue::Null,
        };
        row.insert(col.name.to_lowercase(), typed);
    }
    row
}

/// Evaluate a WHERE expression directly against a RowView without building a Row HashMap.
/// Column values are resolved lazily from the RowView, enabling AND short-circuit
/// to skip parsing columns for rows that fail early predicates.
pub fn evaluate_predicate_view(expr: &Expr, view: &RowView, schema_cols: &[ColumnInfo]) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => eval_binary_op_view(left, op, right, view, schema_cols),
        Expr::UnaryOp { op: UnaryOperator::Not, expr } => {
            !evaluate_predicate_view(expr, view, schema_cols)
        }
        Expr::IsNull(inner) => resolve_value_view(inner, view, schema_cols).is_null(),
        Expr::IsNotNull(inner) => !resolve_value_view(inner, view, schema_cols).is_null(),
        Expr::Between { expr: col, low, high, negated } => {
            let val = resolve_value_view(col, view, schema_cols);
            let lo = resolve_value_view(low, view, schema_cols);
            let hi = resolve_value_view(high, view, schema_cols);
            let in_range = (val.sql_eq(&lo) || val.sql_gt(&lo))
                && (val.sql_eq(&hi) || val.sql_lt(&hi));
            if *negated { !in_range } else { in_range }
        }
        Expr::InList { expr: col, list, negated } => {
            let val = resolve_value_view(col, view, schema_cols);
            let found = list.iter().any(|item| {
                let item_val = resolve_value_view(item, view, schema_cols);
                val.sql_eq(&item_val)
            });
            if *negated { !found } else { found }
        }
        Expr::Like { expr: col_expr, pattern, negated, .. } => {
            let val = resolve_value_view(col_expr, view, schema_cols);
            let pat = resolve_value_view(pattern, view, schema_cols);
            let matches = if let TypedValue::Text(p) = &pat {
                val.sql_like(p)
            } else {
                false
            };
            if *negated { !matches } else { matches }
        }
        Expr::Nested(inner) => evaluate_predicate_view(inner, view, schema_cols),
        _ => false,
    }
}

fn eval_binary_op_view(
    left: &Expr,
    op: &BinaryOperator,
    right: &Expr,
    view: &RowView,
    schema_cols: &[ColumnInfo],
) -> bool {
    match op {
        BinaryOperator::And => {
            evaluate_predicate_view(left, view, schema_cols)
                && evaluate_predicate_view(right, view, schema_cols)
        }
        BinaryOperator::Or => {
            evaluate_predicate_view(left, view, schema_cols)
                || evaluate_predicate_view(right, view, schema_cols)
        }
        _ => {
            let lval = resolve_value_view(left, view, schema_cols);
            let rval = resolve_value_view(right, view, schema_cols);
            match op {
                BinaryOperator::Eq => lval.sql_eq(&rval),
                BinaryOperator::NotEq => !lval.sql_eq(&rval) && !lval.is_null() && !rval.is_null(),
                BinaryOperator::Lt => lval.sql_lt(&rval),
                BinaryOperator::LtEq => lval.sql_lt(&rval) || lval.sql_eq(&rval),
                BinaryOperator::Gt => lval.sql_gt(&rval),
                BinaryOperator::GtEq => lval.sql_gt(&rval) || lval.sql_eq(&rval),
                _ => false,
            }
        }
    }
}

/// Resolve an AST expression to a TypedValue using the RowView's raw data.
fn resolve_value_view(expr: &Expr, view: &RowView, schema_cols: &[ColumnInfo]) -> TypedValue {
    match expr {
        Expr::Identifier(ident) => {
            let col_name = ident.value.to_lowercase();
            if let Some(col) = schema_cols.iter().find(|c| c.name.to_lowercase() == col_name) {
                view.find_col(col.id)
                    .map(|bytes| TypedValue::from_bytes(bytes, &col.data_type))
                    .unwrap_or(TypedValue::Null)
            } else {
                TypedValue::Null
            }
        }
        Expr::CompoundIdentifier(parts) => {
            let col_name = parts.last().map(|p| p.value.to_lowercase()).unwrap_or_default();
            if let Some(col) = schema_cols.iter().find(|c| c.name.to_lowercase() == col_name) {
                view.find_col(col.id)
                    .map(|bytes| TypedValue::from_bytes(bytes, &col.data_type))
                    .unwrap_or(TypedValue::Null)
            } else {
                TypedValue::Null
            }
        }
        Expr::Value(val) => literal_to_typed(val),
        Expr::TypedString { value, .. } => TypedValue::Text(value.clone()),
        Expr::UnaryOp { op: UnaryOperator::Minus, expr } => {
            if let TypedValue::Int(i) = resolve_value_view(expr, view, schema_cols) {
                TypedValue::Int(-i)
            } else if let TypedValue::Float(f) = resolve_value_view(expr, view, schema_cols) {
                TypedValue::Float(-f)
            } else {
                TypedValue::Null
            }
        }
        Expr::BinaryOp { left, op, right } => {
            let lval = resolve_value_view(left, view, schema_cols);
            let rval = resolve_value_view(right, view, schema_cols);
            eval_arithmetic_typed(&lval, op, &rval)
        }
        Expr::Nested(inner) => resolve_value_view(inner, view, schema_cols),
        Expr::Case { operand, conditions, results, else_result, .. } => {
            eval_case_view(
                operand.as_deref(),
                conditions,
                results,
                else_result.as_deref(),
                view,
                schema_cols,
            )
        }
        Expr::Function(f) => {
            let mut resolver = |inner: &Expr| resolve_value_view(inner, view, schema_cols);
            eval_function(f, &mut resolver)
        }
        Expr::Cast { expr, data_type, .. } => {
            let val = resolve_value_view(expr, view, schema_cols);
            val.cast_to_data_type(data_type).unwrap_or(TypedValue::Null)
        }
        Expr::Substring { expr, substring_from, substring_for, .. } => {
            let mut resolver = |inner: &Expr| resolve_value_view(inner, view, schema_cols);
            eval_substring_expr(expr, substring_from.as_deref(), substring_for.as_deref(), &mut resolver)
        }
        Expr::Trim { expr, trim_where, trim_what, trim_characters } => {
            let mut resolver = |inner: &Expr| resolve_value_view(inner, view, schema_cols);
            eval_trim_expr(
                expr,
                trim_where.as_ref(),
                trim_what.as_deref(),
                trim_characters.as_deref(),
                &mut resolver,
            )
        }
        _ => TypedValue::Null,
    }
}

fn eval_case_view(
    operand: Option<&Expr>,
    conditions: &[Expr],
    results: &[Expr],
    else_result: Option<&Expr>,
    view: &RowView,
    schema_cols: &[ColumnInfo],
) -> TypedValue {
    if let Some(op) = operand {
        let op_val = resolve_value_view(op, view, schema_cols);
        for (cond, res) in conditions.iter().zip(results.iter()) {
            let cond_val = resolve_value_view(cond, view, schema_cols);
            if op_val.sql_eq(&cond_val) {
                return resolve_value_view(res, view, schema_cols);
            }
        }
    } else {
        for (cond, res) in conditions.iter().zip(results.iter()) {
            if evaluate_predicate_view(cond, view, schema_cols) {
                return resolve_value_view(res, view, schema_cols);
            }
        }
    }

    else_result
        .map(|expr| resolve_value_view(expr, view, schema_cols))
        .unwrap_or(TypedValue::Null)
}

/// Arithmetic on already-resolved TypedValues (shared between Row and RowView paths).
fn eval_arithmetic_typed(lval: &TypedValue, op: &BinaryOperator, rval: &TypedValue) -> TypedValue {
    if lval.is_null() || rval.is_null() {
        return TypedValue::Null;
    }
    match (lval.as_f64(), rval.as_f64()) {
        (Some(l), Some(r)) => {
            let result = match op {
                BinaryOperator::Plus => l + r,
                BinaryOperator::Minus => l - r,
                BinaryOperator::Multiply => l * r,
                BinaryOperator::Divide => {
                    if r == 0.0 { return TypedValue::Null; }
                    l / r
                }
                BinaryOperator::Modulo => {
                    if r == 0.0 { return TypedValue::Null; }
                    l % r
                }
                _ => return TypedValue::Null,
            };
            if matches!(lval, TypedValue::Int(_))
                && matches!(rval, TypedValue::Int(_))
                && result == (result as i64) as f64
                && !matches!(op, BinaryOperator::Divide)
            {
                TypedValue::Int(result as i64)
            } else {
                TypedValue::Float(result)
            }
        }
        _ => TypedValue::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::SelectItem;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse_where(sql: &str) -> Expr {
        let full = format!("SELECT * FROM t WHERE {sql}");
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, &full).unwrap();
        if let sqlparser::ast::Statement::Query(q) = &stmts[0] {
            if let sqlparser::ast::SetExpr::Select(sel) = q.body.as_ref() {
                return sel.selection.clone().unwrap();
            }
        }
        panic!("failed to parse WHERE clause");
    }

    fn parse_select_expr(sql: &str) -> Expr {
        let full = format!("SELECT {sql} FROM t");
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, &full).unwrap();
        if let sqlparser::ast::Statement::Query(q) = &stmts[0] {
            if let sqlparser::ast::SetExpr::Select(sel) = q.body.as_ref() {
                if let SelectItem::UnnamedExpr(expr) = &sel.projection[0] {
                    return expr.clone();
                }
            }
        }
        panic!("failed to parse SELECT expression");
    }

    fn test_schema() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo { id: 1, name: "id".into(), data_type: "BIGINT".into(), nullable: false, is_pk: true, table_id: 1, indexed: false, index_name: None },
            ColumnInfo { id: 2, name: "name".into(), data_type: "VARCHAR(255)".into(), nullable: false, is_pk: false, table_id: 1, indexed: false, index_name: None },
            ColumnInfo { id: 3, name: "age".into(), data_type: "INT".into(), nullable: true, is_pk: false, table_id: 1, indexed: false, index_name: None },
        ]
    }

    fn test_row() -> Row {
        let mut row = Row::new();
        row.insert("id".into(), TypedValue::Int(1));
        row.insert("name".into(), TypedValue::Text("alice".into()));
        row.insert("age".into(), TypedValue::Int(30));
        row
    }

    #[test]
    fn eq_predicate() {
        let expr = parse_where("id = 1");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn neq_predicate() {
        let expr = parse_where("id != 2");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn lt_predicate() {
        let expr = parse_where("age < 40");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn gt_predicate() {
        let expr = parse_where("age > 20");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn and_predicate() {
        let expr = parse_where("age > 20 AND age < 40");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn or_predicate() {
        let expr = parse_where("age = 99 OR name = 'alice'");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn between_predicate() {
        let expr = parse_where("age BETWEEN 25 AND 35");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn not_between() {
        let expr = parse_where("age NOT BETWEEN 31 AND 50");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn in_list_predicate() {
        let expr = parse_where("age IN (28, 30, 32)");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn not_in_list() {
        let expr = parse_where("age NOT IN (28, 29, 31)");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn is_null_predicate() {
        let mut row = test_row();
        row.insert("age".into(), TypedValue::Null);
        let expr = parse_where("age IS NULL");
        assert!(evaluate_predicate(&expr, &row, &test_schema()));
    }

    #[test]
    fn is_not_null_predicate() {
        let expr = parse_where("age IS NOT NULL");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn like_predicate() {
        let expr = parse_where("name LIKE 'ali%'");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn not_like_predicate() {
        let expr = parse_where("name NOT LIKE 'bob%'");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn bytes_to_row_works() {
        let schema = test_schema();
        let cols = vec![
            (&schema[0], Some(b"42".to_vec())),
            (&schema[1], Some(b"alice".to_vec())),
            (&schema[2], None),
        ];
        let row = bytes_to_row(&cols);
        assert_eq!(row["id"], TypedValue::Int(42));
        assert_eq!(row["name"], TypedValue::Text("alice".into()));
        assert_eq!(row["age"], TypedValue::Null);
    }

    #[test]
    fn arithmetic_multiply_in_where() {
        let expr = parse_where("id * age > 20");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn arithmetic_add_in_where() {
        let expr = parse_where("id + age = 31");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn arithmetic_subtract_in_where() {
        let expr = parse_where("age - id = 29");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn arithmetic_nested_expr() {
        // age * (id + 1) = 30 * (1+1) = 60
        let expr = parse_where("age * (id + 1) = 60");
        assert!(evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn arithmetic_resolve_value() {
        let full = "SELECT id * age FROM t WHERE id = 1";
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, full).unwrap();
        if let sqlparser::ast::Statement::Query(q) = &stmts[0] {
            if let sqlparser::ast::SetExpr::Select(sel) = q.body.as_ref() {
                if let SelectItem::UnnamedExpr(ref expr) = sel.projection[0] {
                    let val = resolve_value(expr, &test_row(), &test_schema());
                    assert_eq!(val, TypedValue::Int(30)); // 1 * 30
                    return;
                }
            }
        }
        panic!("failed to parse");
    }

    #[test]
    fn arithmetic_division_by_zero() {
        let expr = parse_where("age / 0 = 1");
        assert!(!evaluate_predicate(&expr, &test_row(), &test_schema()));
    }

    #[test]
    fn coalesce_prefers_first_non_null_value() {
        let mut row = test_row();
        row.insert("name".into(), TypedValue::Null);
        let expr = parse_select_expr("COALESCE(name, 'fallback')");
        assert_eq!(resolve_value(&expr, &row, &test_schema()), TypedValue::Text("fallback".into()));
    }

    #[test]
    fn nullif_returns_null_for_equal_values() {
        let expr = parse_select_expr("NULLIF(name, 'alice')");
        assert_eq!(resolve_value(&expr, &test_row(), &test_schema()), TypedValue::Null);
    }

    #[test]
    fn text_functions_and_substring_work() {
        let expr = parse_select_expr("SUBSTRING(UPPER(TRIM(name)) FROM 2 FOR 3)");
        assert_eq!(resolve_value(&expr, &test_row(), &test_schema()), TypedValue::Text("LIC".into()));
    }

    #[test]
    fn cast_and_current_date_expressions_work() {
        let cast_expr = parse_select_expr("CAST(age AS TEXT)");
        assert_eq!(resolve_value(&cast_expr, &test_row(), &test_schema()), TypedValue::Text("30".into()));

        let date_expr = parse_select_expr("CURRENT_DATE");
        match resolve_value(&date_expr, &test_row(), &test_schema()) {
            TypedValue::Text(date) => assert_eq!(date.len(), 10),
            other => panic!("expected CURRENT_DATE text, got {:?}", other),
        }
    }
}
